//! # Anachro QSPI
//!
//! ## Notes
//!
//! This is not a general purpose library!
//!
//! Chip: W25Q128JVSIQ
//! note: This chip defaults to quad mode enabled!
//!
//! Manufacturer ID: 0xEF
//! Device ID: 0x17_4018
//!
//! * 256-byte pages (writeable) - 65536x
//! * 4KiB sectors (eraseable) - 4096x
//! * 32KiB Small-Blocks (eraseable) - 512x
//! * 64KiB Large-Blocks (eraseable) - 256x
//! * 16MiB Whole Chip (eraseable)
//!
//! What do I need the flash chip for?
//!
//! * Storing firmware images - Up to 1MiB each
//! * Keeping a rolling buffer of log messages?
//!     * Device has a lifetime of 100K prog/erase cycles
//!     * At 8Mib for logs, how long would it take to generate 781.25 GiB of log data?
//!
//! Okay, let's plan out a memory map
//!
//! MB - ADDR Range
//! ===========================================
//! 00 - 0x000000 - 0x0FFFFF: Config
//! 01 - 0x100000 - 0x1FFFFF:
//! 02 - 0x200000 - 0x2FFFFF:
//! 03 - 0x300000 - 0x3FFFFF:
//! 04 - 0x400000 - 0x4FFFFF: Firmware Slot 1
//! 05 - 0x500000 - 0x5FFFFF: Firmware Slot 2
//! 06 - 0x600000 - 0x6FFFFF:
//! 07 - 0x700000 - 0x7FFFFF:
//! 08 - 0x800000 - 0x8FFFFF: A4 Slot 1
//! 09 - 0x900000 - 0x9FFFFF: A4 Slot 2
//! 10 - 0xA00000 - 0xAFFFFF:
//! 11 - 0xB00000 - 0xBFFFFF:
//! 12 - 0xC00000 - 0xCFFFFF:
//! 13 - 0xD00000 - 0xDFFFFF:
//! 14 - 0xE00000 - 0xEFFFFF:
//! 15 - 0xF00000 - 0xFFFFFF:

#![cfg_attr(not(test), no_std)]

use core::{sync::atomic::Ordering, task::Poll, ops::Deref};
pub use byte_slab::ManagedArcSlab;

pub const QSPI_MAPPED_BASE_ADDRESS: usize = 0x12000000;
pub const QSPI_LOCAL_FIRMWARE_SLOT_1: usize = 4 * 1024 * 1024;
pub const QSPI_MAPPED_FIRMWARE_SLOT_1: usize = QSPI_MAPPED_BASE_ADDRESS + QSPI_LOCAL_FIRMWARE_SLOT_1;
pub const QSPI_LOCAL_FIRMWARE_SLOT_2: usize = 5 * 1024 * 1024;
pub const QSPI_MAPPED_FIRMWARE_SLOT_2: usize = QSPI_MAPPED_BASE_ADDRESS + QSPI_LOCAL_FIRMWARE_SLOT_2;

pub struct FlashChunk<'a, const CT: usize, const SZ: usize> {
    pub addr: usize,
    pub data: ManagedArcSlab<'a, CT, SZ>,
}

use cassette::futures::poll_fn;
use nrf52840_hal::{
    gpio::{Disconnected, Pin, Port},
    pac::{P0, P1, QSPI},
};

pub use nrf52840_hal::pac::qspi::erase::len::LEN_A as EraseLength;

pub struct QspiPins {
    pub qspi_copi_io0: Pin<Disconnected>,
    pub qspi_cipo_io1: Pin<Disconnected>,
    pub qspi_io2: Pin<Disconnected>, // Also "WPn"
    pub qspi_io3: Pin<Disconnected>, // Also "HOLDn"
    pub qspi_csn: Pin<Disconnected>,
    pub qspi_sck: Pin<Disconnected>,
}

pub struct Qspi {
    _pins: QspiPins,
    periph: QSPI,
}

#[derive(defmt::Format)]
pub enum Error {
    /// Address was not aligned properly
    Alignment,
}

impl Qspi {
    pub fn new(periph: QSPI, pins: QspiPins) -> Self {
        core::sync::atomic::compiler_fence(Ordering::SeqCst);
        periph.enable.write(|w| w.enable().disabled());
        // 0x40029054ul
        unsafe {
            (0x40029054u32 as *mut u32).write_volatile(0x0000_0001);
        }
        core::sync::atomic::compiler_fence(Ordering::SeqCst);

        let pinarr = &[
            &pins.qspi_copi_io0,
            &pins.qspi_cipo_io1,
            &pins.qspi_io2,
            &pins.qspi_io3,
            &pins.qspi_csn,
            &pins.qspi_sck,
        ];

        for pin in pinarr {
            let port = match pin.port() {
                Port::Port0 => unsafe { &*P0::ptr() },
                Port::Port1 => unsafe { &*P1::ptr() },
            };

            port.pin_cnf[pin.pin() as usize].modify(|_r, w| w.drive().h0h1());
        }

        unsafe {
            periph.psel.io0.write(|w| {
                w.bits(pins.qspi_copi_io0.psel_bits());
                w.connect().connected()
            });
            periph.psel.io1.write(|w| {
                w.bits(pins.qspi_cipo_io1.psel_bits());
                w.connect().connected()
            });
            periph.psel.io2.write(|w| {
                w.bits(pins.qspi_io2.psel_bits());
                w.connect().connected()
            });
            periph.psel.io3.write(|w| {
                w.bits(pins.qspi_io3.psel_bits());
                w.connect().connected()
            });
            periph.psel.csn.write(|w| {
                w.bits(pins.qspi_csn.psel_bits());
                w.connect().connected()
            });
            periph.psel.sck.write(|w| {
                w.bits(pins.qspi_sck.psel_bits());
                w.connect().connected()
            });
        }

        // periph.ifconfig0.write(|w| {
        //     w.readoc().read4io();
        //     w.writeoc().pp4o();
        //     w.addrmode()._24bit();
        //     w.dpmenable().disable();
        //     w.ppsize()._256bytes();
        //     w
        // });
        periph.ifconfig0.write(|w| {
            w.readoc().fastread();
            w.writeoc().pp();
            w.addrmode()._24bit();
            w.dpmenable().disable();
            w.ppsize()._256bytes();
            w
        });
        periph.ifconfig1.write(|w| {
            // One 16-mhz cycle delay. As far as I can tell, we don't
            // even need this?
            unsafe { w.sckdelay().bits(1) };
            w.dpmen().exit();
            w.spimode().mode0();
            // SPI freqency of 32mhz
            unsafe { w.sckfreq().bits(15) };
            w
        });

        // Enable QSPI peripheral
        periph.enable.write(|w| w.enable().enabled());

        // Clear the "is ready" flag
        periph.events_ready.reset();

        // Activate the flash device
        periph
            .tasks_activate
            .write(|w| w.tasks_activate().set_bit());

        // Wait for the ready flag
        while periph.events_ready.read().events_ready().bit_is_clear() {}

        // Make sure no reads happen BEFORE the QSPI is enabled
        core::sync::atomic::compiler_fence(Ordering::SeqCst);

        Self {
            _pins: pins,
            periph,
        }
    }


    pub fn read_slice(&self, flash_addr: usize, len: usize) -> Result<&[u8], u32> {
        if !(flash_addr < (16 * 1024 * 1024)) {
            return Err(flash_addr as u32);
        }
        if !(flash_addr + len) <= (16 * 1024 * 1024) {
            return Err((flash_addr + len + 1) as u32);
        }

        core::sync::atomic::compiler_fence(Ordering::SeqCst);
        Ok(unsafe {
            core::slice::from_raw_parts(
                (flash_addr + QSPI_MAPPED_BASE_ADDRESS) as *const u8,
                len
            )
        })
    }

    pub async fn read(&mut self, start: usize, dest: &mut [u8]) -> Result<(), Error> {
        core::sync::atomic::compiler_fence(Ordering::SeqCst);

        self.periph.read.dst.write(|w| unsafe { w.bits(dest.as_ptr() as u32) });
        self.periph.read.src.write(|w| unsafe { w.bits(start as u32)});
        self.periph.read.cnt.write(|w| unsafe { w.bits(dest.len() as u32)});

        core::sync::atomic::compiler_fence(Ordering::SeqCst);
        self.periph.events_ready.reset();
        self.periph.tasks_readstart.write(|w| w.tasks_readstart().set_bit());
        core::sync::atomic::compiler_fence(Ordering::SeqCst);
        self.wait_done().await;
        core::sync::atomic::compiler_fence(Ordering::SeqCst);

        Ok(())
    }

    pub async fn write<'a, const CT: usize, const SZ: usize>(&mut self, data: FlashChunk<'a, CT, SZ>) -> Result<(), Error> {
        core::sync::atomic::compiler_fence(Ordering::SeqCst);

        self.periph.write.dst.write(|w| unsafe { w.bits(data.addr as u32)});
        self.periph.write.src.write(|w| unsafe { w.bits(data.data.deref().as_ptr() as u32)});
        self.periph.write.cnt.write(|w| unsafe { w.bits(data.data.len() as u32)});

        core::sync::atomic::compiler_fence(Ordering::SeqCst);
        self.periph.events_ready.reset();
        self.periph.tasks_writestart.write(|w| w.tasks_writestart().set_bit());
        core::sync::atomic::compiler_fence(Ordering::SeqCst);
        self.wait_done().await;
        core::sync::atomic::compiler_fence(Ordering::SeqCst);

        drop(data);

        Ok(())
    }

    pub async fn erase(&mut self, start: usize, len: EraseLength) -> Result<(), Error> {
        core::sync::atomic::compiler_fence(Ordering::SeqCst);

        // Ensure alignment to page size
        match len {
            EraseLength::_4KB if start & 0xFFF != 0 => return Err(Error::Alignment),
            EraseLength::_64KB if start & 0xFFFF != 0 => return Err(Error::Alignment),
            EraseLength::ALL if start != 0 => return Err(Error::Alignment),
            _ => {}
        }

        self.periph.erase.ptr.write(|w| unsafe { w.bits(start as u32) });
        self.periph.erase.len.write(|w| w.len().variant(len) );

        core::sync::atomic::compiler_fence(Ordering::SeqCst);
        self.periph.events_ready.reset();
        self.periph.tasks_erasestart.write(|w| w.tasks_erasestart().set_bit());
        core::sync::atomic::compiler_fence(Ordering::SeqCst);
        self.wait_done().await;
        core::sync::atomic::compiler_fence(Ordering::SeqCst);

        Ok(())
    }

    pub async fn wait_done(&self) {
        poll_fn(|_| {
            if self.periph.events_ready.read().events_ready().bit_is_clear() {
                Poll::Pending
            } else {
                Poll::Ready(())
            }
        }).await
    }

    pub fn uninit(self) {
        core::sync::atomic::compiler_fence(Ordering::SeqCst);
        // self.periph.tasks_deactivate.write(|w| w.tasks_deactivate().set_bit());
        self.periph.enable.write(|w| w.enable().disabled());
        unsafe {
            (0x40029054u32 as *mut u32).write_volatile(0x0000_0001);
        }
        core::sync::atomic::compiler_fence(Ordering::SeqCst);
        // TODO: How to delay? This doesn't cause a ready event
    }
}

