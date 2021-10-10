#![no_main]
#![no_std]

#![allow(unused_imports)]

use core::ptr::NonNull;

use anachro_boot::{self as _, bootdata::Bootdata, bootload::{Metadata, Nvmc, PartialStatus, UsableSections}, consts::{PAGE_SIZE, SECTION_1_START_APP, SECTION_1_START_METADATA}}; // global logger + panicking-behavior + memory layout
use nrf52840_hal::pac::Peripherals;
use groundhog::RollingTimer;
use groundhog_nrf52::GlobalRollingTimer;

static BOOT_1_IMG: &[u8] = include_bytes!("../../test-images/blink_1_app_combo.bin");
// static BOOT_2_IMG: &[u8] = include_bytes!("../../test-images/blink_2_app_combo.bin");

#[cortex_m_rt::entry]
fn main() -> ! {
    defmt::info!("Hello, world!");
    let board = defmt::unwrap!(Peripherals::take());
    let timer = GlobalRollingTimer::default();

    GlobalRollingTimer::init(board.TIMER0);

    let nvmc = board.NVMC;

    let mut us1 = Nvmc::new(&nvmc, UsableSections::Section1);

    let start = timer.get_ticks();
    defmt::info!("Pause...");
    while timer.millis_since(start) < 500 { }
    defmt::info!("Begin!...");

    assert_eq!(BOOT_1_IMG.as_ptr() as usize & 0b11, 0);
    assert_eq!(BOOT_1_IMG.len() & (PAGE_SIZE - 1), 0);

    for (i_pg, page) in BOOT_1_IMG.chunks_exact(PAGE_SIZE).enumerate() {
        defmt::info!("Starting erase of page {=usize}", i_pg);
        defmt::unwrap!(us1.start_partial_erase(1, 0x40_000 + (i_pg * PAGE_SIZE)));

        loop {
            let start = timer.get_ticks();
            match us1.step_partial_erase() {
                Ok(PartialStatus::Done) => break,
                Ok(PartialStatus::RemainingMs(_)) => {
                    while timer.millis_since(start) < 5 { }
                }
                Err(_) => defmt::todo!(),
            }
        }

        defmt::info!("Finished erase.");

        let mut write_start = timer.get_ticks();
        us1.enable_write();

        defmt::info!("Starting write...");

        for (i_by, wrch) in page.chunks_exact(4).enumerate() {
            let mut wby = [0u8; 4];
            wby.copy_from_slice(wrch);
            let word = u32::from_le_bytes(wby);

            let page = 0x40_000 + (i_pg * PAGE_SIZE);
            let byts = i_by * 4;
            let addr = page + byts;

            us1.write_word(addr, word);

            if timer.ticks_since(write_start) > 1_000 {
                us1.enable_read();
                while timer.ticks_since(write_start) < 5000 { }
                us1.enable_write();
                write_start = timer.get_ticks();
            }
        }

        defmt::info!("Write complete!");
    }

    defmt::info!("Writing bootdata...");

    let bd = Bootdata {
        app_metadata: defmt::unwrap!(NonNull::new(0x40_000 as *const u8 as *mut u8)),
        own_metadata: defmt::unwrap!(NonNull::new(0x80_000 as *const u8 as *mut u8)),
        nxt_metadata: defmt::unwrap!(NonNull::new(0xC0_000 as *const u8 as *mut u8)),
        nxt_image: defmt::unwrap!(NonNull::new(0xC1_000 as *const u8 as *mut u8)),
        is_first_boot: true,
        is_rollback: false,
    };

    defmt::unwrap!(bd.write_to(0x2003FC00));

    defmt::info!("Fingers crossed, bootloading in 1s!");

    let start = timer.get_ticks();
    while timer.millis_since(start) <= 1000 { }

    unsafe {
        cortex_m::asm::bootload(0x41000 as *const _);
    }
}
