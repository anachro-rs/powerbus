#![cfg_attr(not(test), no_std)]
#![allow(unused_imports, dead_code)]

use core::{
    ops::{DerefMut, Deref},
    sync::atomic::{compiler_fence, Ordering::SeqCst},
};

use anachro_485::{dispatch::{IoHandle, TimeStampBox}, icd::{SLAB_SIZE, TOTAL_SLABS}};
use byte_slab::{BSlab, ManagedArcSlab, SlabBox};
use nrf52840_hal::{gpio::{Disconnected, Floating, Input, Level, Output, Pin, PushPull}, pac::{Interrupt, TIMER0, TIMER1, TIMER2, TIMER3, TIMER4, UARTE0, UARTE1}, ppi::{ConfigurablePpi, Ppi}, prelude::OutputPin, target_constants::EASY_DMA_SIZE, timer::Instance as TimerInstance, uarte::{Baudrate, Instance as UarteInstance, Parity, Pins}};
use defmt::{info, warn, error};

type UarteBSlab = BSlab<TOTAL_SLABS, SLAB_SIZE>;
type UarteBox = SlabBox<TOTAL_SLABS, SLAB_SIZE>;
type UarteMas = ManagedArcSlab<'static, TOTAL_SLABS, SLAB_SIZE>;

pub struct Uarte485<Timer, Channel, Uarte>
where
    Timer: TimerInstance,
    Channel: Ppi + ConfigurablePpi,
    Uarte: Uarte485Instance,
{
    alloc: &'static UarteBSlab,
    timer: Timer,
    channel: Channel,
    uarte: Uarte,
    pins: InternalPin485,
    state: State485,
    io_hdl: IoHandle,
    has_rx_credit: bool,
    timeout: bool,
}

enum State485 {
    Idle,
    RxAwaitFirstByte(UarteBox),
    RxReceiving(UarteBox),
    TxSending(UarteMas),
    Invalid,
}

impl State485 {
    fn log_state(&self) {
        match self {
            State485::Idle => defmt::trace!("state: Idle"),
            State485::RxAwaitFirstByte(_) => defmt::trace!("state: RxAwaitFirstByte"),
            State485::RxReceiving(_) => defmt::trace!("state: RxReceiving"),
            State485::TxSending(_) => defmt::trace!("state: TxSending"),
            State485::Invalid => defmt::trace!("state: Invalid"),
        }
    }
}

pub struct Pin485 {
    pub rs_di: Pin<Disconnected>,
    pub rs_ro: Pin<Disconnected>,
    pub rs_de: Pin<Disconnected>,
    pub rs_re_n: Pin<Disconnected>,
}

pub struct InternalPin485 {
    rs_de: Pin<Output<PushPull>>,
    rs_re_n: Pin<Output<PushPull>>,

    // Don't actually use these two! They are used by the UARTE!
    _rs_di: Pin<Output<PushPull>>,
    _rs_ro: Pin<Input<Floating>>,
}

enum Again {
    No,
    Yes,
}

impl<Timer, Channel, Uarte> Uarte485<Timer, Channel, Uarte>
where
    Timer: TimerInstance,
    Channel: Ppi + ConfigurablePpi,
    Uarte: Uarte485Instance,
{
    pub fn new(
        alloc: &'static UarteBSlab,
        timer: Timer,
        mut channel: Channel,
        uarte: Uarte,
        pins: Pin485,
        ioh: IoHandle,
    ) -> Self {
        let pins = InternalPin485 {
            rs_de: pins.rs_de.into_push_pull_output(Level::Low),
            rs_re_n: pins.rs_re_n.into_push_pull_output(Level::High),
            _rs_di: pins.rs_di.into_push_pull_output(Level::High),
            _rs_ro: pins.rs_ro.into_floating_input(),
        };

        {
            // Setup pins
            uarte.psel.rxd.write(|w| {
                let w = unsafe { w.bits(pins._rs_ro.psel_bits()) };
                w.connect().connected()
            });
            uarte.psel.txd.write(|w| {
                let w = unsafe { w.bits(pins._rs_di.psel_bits()) };
                w.connect().connected()
            });
            uarte.psel.cts.write(|w| w.connect().disconnected());
            uarte.psel.rts.write(|w| w.connect().disconnected());

            // Enable + Config UARTE
            uarte.enable.write(|w| w.enable().enabled());
            uarte.config.write(|w| {
                w.hwfc().clear_bit();
                w.parity().variant(Parity::EXCLUDED);
                w
            });

            // TODO: Variable baudrate?
            uarte.baudrate.write(|w| w.baudrate().baud1m());
            uarte.intenclr.write(|w| unsafe { w.bits(0xFFFF_FFFF) });
        }

        timer.set_oneshot();

        // Set up PPI shortcut to reset the timeout timer on every byte received
        {
            let hw_timer = match Timer::INTERRUPT {
                Interrupt::TIMER0 => TIMER0::ptr(),
                Interrupt::TIMER1 => TIMER1::ptr(),
                Interrupt::TIMER2 => TIMER2::ptr(),
                Interrupt::TIMER3 => TIMER3::ptr().cast(), // double yolo
                Interrupt::TIMER4 => TIMER4::ptr().cast(), // double yolo
                _ => unreachable!(),
            };

            channel.set_task_endpoint(unsafe { &(&*hw_timer).tasks_clear });
            channel.set_event_endpoint(&uarte.events_rxdrdy);
        }

        Self {
            alloc,
            timer,
            channel,
            uarte,
            pins,
            state: State485::Idle,
            io_hdl: ioh,
            has_rx_credit: false,
            timeout: false,
        }
    }

    pub fn debug_events(&self) {
        if self.uarte.events_rxdrdy.read().events_rxdrdy().bit_is_set() {
            defmt::info!("rxdrdy")
        }
        if self.uarte.events_endtx.read().events_endtx().bit_is_set() {
            defmt::info!("endtx")
        }
        if self.uarte.events_txstopped.read().events_txstopped().bit_is_set() {
            defmt::info!("txstopped")
        }
        if self.uarte.events_endrx.read().events_endrx().bit_is_set() {
            defmt::info!("endrx")
        }
    }

    pub fn prepare_send(&mut self, msg: &UarteMas) {
        defmt::assert!(EASY_DMA_SIZE >= msg.len());

        // GPIOs
        {
            self.pins.rs_re_n.set_high().ok();
            self.pins.rs_de.set_high().ok();
        }

        // TIMERs
        {
            self.timer.disable_interrupt();
            self.timer.timer_cancel();
            self.timer.timer_reset_event();
            self.channel.disable();
        }

        // UARTE
        {
            // Conservative compiler fence to prevent optimizations that do not
            // take in to account actions by DMA. The fence has been placed here,
            // before any DMA action has started
            compiler_fence(SeqCst);

            let tx_buffer: &[u8] = msg.deref();

            // Reset the events.
            self.uarte.events_endtx.reset();
            self.uarte.events_txstopped.reset();
            self.uarte.intenset.write(|w| w.endtx().set_bit());

            // Set up the DMA write
            self.uarte.txd.ptr.write(|w|
                // We're giving the register a pointer to the stack. Since we're
                // waiting for the UARTE transaction to end before this stack pointer
                // becomes invalid, there's nothing wrong here.
                //
                // The PTR field is a full 32 bits wide and accepts the full range
                // of values.
                unsafe { w.ptr().bits(tx_buffer.as_ptr() as u32) });
            self.uarte.txd.maxcnt.write(|w|
                // We're giving it the length of the buffer, so no danger of
                // accessing invalid memory. We have verified that the length of the
                // buffer fits in an `u8`, so the cast to `u8` is also fine.
                //
                // The MAXCNT field is 8 bits wide and accepts the full range of
                // values.
                unsafe { w.maxcnt().bits(tx_buffer.len() as _) });

            // Start UARTE Transmit transaction
            self.uarte.tasks_starttx.write(|w|
                // `1` is a valid value to write to task registers.
                unsafe { w.bits(1) });
        }
    }

    pub fn complete_send(&mut self, msg: &UarteMas) -> Result<(), ()> {
        let endtx = self.uarte.events_endtx.read().events_endtx().bit_is_set();
        if !endtx {
            return Err(());
        }

        let sent = self.uarte.txd.amount.read().amount().bits() as usize;
        defmt::assert_eq!(sent, msg.len());

        self.uarte.intenclr.write(|w| w.endtx().set_bit());
        self.uarte.events_endtx.reset();

        self.pins.rs_re_n.set_high().ok();
        self.pins.rs_de.set_low().ok();

        Ok(())
    }

    pub fn prepare_recv_initial(&mut self, sbox: &mut UarteBox) {
        defmt::assert!(EASY_DMA_SIZE >= SLAB_SIZE);

        // Manage timer
        {
            self.timer.disable_interrupt();
            self.timer.timer_cancel();
            self.timer.timer_reset_event();
            self.channel.disable();
        }

        // Manage timer
        {
            // This is the timer that triggers when idle
            self.timer.enable_interrupt();

            // TODO: Don't hardcode 1000us
            self.timer.timer_start(1000u32);
        }

        // Manage gpios
        {
            self.pins.rs_de.set_low().ok();
            self.pins.rs_re_n.set_low().ok();
        }

        // Manage Uarte
        {
            let rx_buffer: &mut [u8] = sbox.deref_mut();

            // NOTE: RAM slice check is not necessary, as a mutable slice can only be
            // built from data located in RAM

            // Conservative compiler fence to prevent optimizations that do not
            // take in to account actions by DMA. The fence has been placed here,
            // before any DMA action has started
            compiler_fence(SeqCst);

            // Set up the DMA read
            self.uarte.rxd.ptr.write(|w|
                // We're giving the register a pointer to the stack. Since we're
                // waiting for the UARTE transaction to end before this stack pointer
                // becomes invalid, there's nothing wrong here.
                //
                // The PTR field is a full 32 bits wide and accepts the full range
                // of values.
                unsafe { w.ptr().bits(rx_buffer.as_ptr() as u32) });
            self.uarte.rxd.maxcnt.write(|w|
                // We're giving it the length of the buffer, so no danger of
                // accessing invalid memory. We have verified that the length of the
                // buffer fits in an `u8`, so the cast to `u8` is also fine.
                //
                // The MAXCNT field is at least 8 bits wide and accepts the full
                // range of values.
                unsafe { w.maxcnt().bits(rx_buffer.len() as _) });

            self.uarte.intenset.write(|w| w.rxdrdy().set_bit());
            self.uarte.events_rxdrdy.write(|w| w.events_rxdrdy().clear_bit());
            self.uarte.intenclr.write(|w| w.endrx().set_bit());

            // Start UARTE Receive transaction
            self.uarte.tasks_startrx.write(|w|
                // `1` is a valid value to write to task registers.
                w.tasks_startrx().set_bit()
            );
        }
    }

    pub fn prepare_steady_recv(&mut self) -> Result<(), ()> {
        if self.uarte.events_rxdrdy.read().events_rxdrdy().bit_is_clear() {
            return Err(());
        }
        defmt::trace!("done.");
        // Manage Uarte
        {
            self.uarte.intenclr.write(|w| w.rxdrdy().set_bit());
            self.uarte.events_endrx.write(|w| w.events_endrx().clear_bit());
            self.uarte.intenset.write(|w| w.endrx().set_bit());
        }

        // Manage timer
        {
            self.timer.timer_cancel();

            // This is the timer that triggers when idle
            self.timer.enable_interrupt();

            // TODO: Don't hardcode 100us
            self.timer.timer_start(100u32);

            // This resets the timer every time we get a byte
            // Each byte takes ~9uS at 1mbit, so this is 10 or so
            // "quiet bytes" to allow a full timeout
            self.channel.enable();
        }

        Ok(())
    }

    pub fn complete_recv(&mut self, sbox: UarteBox) {

        {
            self.timer.disable_interrupt();
            self.timer.timer_cancel();
            self.channel.disable();
        }

        {
            if !self.uarte.events_endrx.read().events_endrx().bit_is_set() {
                defmt::trace!("A timeout!");

                // We hit here because of a timeout!
                self.uarte.events_rxto.write(|w| w);

                // Stop reception
                self.uarte.tasks_stoprx.write(|w| unsafe { w.bits(1) });

                // Wait for the reception to have stopped
                while self.uarte.events_rxto.read().bits() == 0 {}
            }

            // Disable endrx interrupt
            self.uarte.intenclr.write(|w| w.endrx().set_bit());
            self.uarte.events_endrx.write(|w| w.events_endrx().clear_bit());

            // Reset the event flag
            self.uarte.events_rxto.write(|w| w);

            // Ask UART to flush FIFO to DMA buffer
            self.uarte.tasks_flushrx.write(|w| unsafe { w.bits(1) });

            // Wait for the flush to complete.
            while self.uarte.events_endrx.read().bits() == 0 {}
            self.uarte.events_endrx.write(|w| w.events_endrx().clear_bit());

            compiler_fence(SeqCst);

            let len = self.uarte.rxd.amount.read().bits() as usize;

            if len != 0 {
                let result = self.io_hdl.push_incoming(TimeStampBox {
                    packet: sbox,
                    len,
                    tick: 0, // TODO
                });

                defmt::assert!(result.is_ok());
            } else {
                defmt::warn!("Zero Size Packet!");
            }
        }
    }



    pub fn uarte_interrupt(&mut self) {
        loop {
            if let Again::No = self.uarte_interrupt_inner() {
                break;
            } else {
                self.state.log_state();
            }
        }
        self.state.log_state();
    }

    fn uarte_interrupt_inner(&mut self) -> Again {
        // self.debug_events();
        let mut again = Again::No;
        let mut old_state = State485::Invalid;
        core::mem::swap(&mut old_state, &mut self.state);

        self.state = match old_state {
            State485::Idle if self.has_rx_credit && self.io_hdl.auth().is_send_authd() => {
                if let Some(msg) = self.io_hdl.pop_outgoing() {
                    self.io_hdl.auth().clear_send_auth();
                    self.has_rx_credit = false;
                    self.prepare_send(&msg);
                    State485::TxSending(msg)
                } else {
                    defmt::panic!("Send authorized but no packet?")
                }
            },
            State485::Idle if self.io_hdl.auth().is_recv_authd() => {
                if let Some(mut sbox) = self.alloc.alloc_box() {
                    self.prepare_recv_initial(&mut sbox);
                    State485::RxAwaitFirstByte(sbox)
                } else {
                    defmt::panic!("Recv requested but no alloc!");
                }
            },
            State485::Idle => {
                warn!("Idle to idle transition? Why?");
                State485::Idle
            },
            State485::RxAwaitFirstByte(sbox) if self.timeout => {
                drop(sbox);
                again = Again::Yes;
                warn!("Initial RX timeout");
                self.timer.disable_interrupt();
                self.timer.timer_cancel();
                State485::Idle
            }
            State485::RxAwaitFirstByte(sbox) => {
                match self.prepare_steady_recv() {
                    Ok(_) => State485::RxReceiving(sbox),
                    Err(_) => {
                        State485::RxAwaitFirstByte(sbox)
                    }
                }

            },
            State485::RxReceiving(sbox) => {
                self.complete_recv(sbox);
                defmt::info!("Done receiving.");
                self.has_rx_credit = true;
                again = Again::Yes;
                State485::Idle
            },
            State485::TxSending(msg) => {
                match self.complete_send(&msg) {
                    Ok(_) => {
                        again = Again::Yes;
                        defmt::info!("Done sending.");
                        State485::Idle
                    }
                    Err(_) => {
                        State485::TxSending(msg)
                    }
                }

            },
            State485::Invalid => {
                defmt::panic!("Invalid state in Uarte485!");
            },
        };

        self.timeout = false;

        again
    }

    pub fn timer_interrupt(&mut self) {
        self.timeout = true;
    }
}

pub trait Uarte485Instance: UarteInstance {
    const INTERRUPT: Interrupt;
}

impl Uarte485Instance for UARTE0 {
    const INTERRUPT: Interrupt = Interrupt::UARTE0_UART0;
}

impl Uarte485Instance for UARTE1 {
    const INTERRUPT: Interrupt = Interrupt::UARTE1;
}