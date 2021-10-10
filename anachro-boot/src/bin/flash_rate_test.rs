#![no_main]
#![no_std]

#![allow(unused_imports)]

use core::ptr::NonNull;

use anachro_boot::{self as _, bootload::{Metadata, Nvmc, PartialStatus, UsableSections}, consts::{PAGE_SIZE, SECTION_1_START_APP, SECTION_1_START_METADATA}}; // global logger + panicking-behavior + memory layout
use nrf52840_hal::pac::Peripherals;
use groundhog::RollingTimer;
use groundhog_nrf52::GlobalRollingTimer;

#[cortex_m_rt::entry]
fn main() -> ! {
    defmt::info!("Hello, world!");
    let board = defmt::unwrap!(Peripherals::take());
    let timer = GlobalRollingTimer::default();

    GlobalRollingTimer::init(board.TIMER0);

    let nvmc = board.NVMC;

    let mut us1 = Nvmc::new(&nvmc, UsableSections::Section1);

    defmt::info!("Pause...");
    let start = timer.get_ticks();
    while timer.millis_since(start) < 1000 { }

    defmt::info!("Erasing metadata...");

    let start = timer.get_ticks();
    us1.enable_erase();
    us1.erase_metadata();
    us1.enable_read();

    let end = timer.ticks_since(start);
    // Expermentally: 84kticks
    defmt::info!("Erase took {=u32} ticks", end);

    // Write something to the page
    defmt::info!("Writing...");
    let start = timer.get_ticks();
    us1.enable_write();
    for i in 0..(PAGE_SIZE as u32 / 4) {
        let word = i | (i << 8) | (i << 16) | (i << 24);
        let offset = (i * 4) as usize;
        us1.write_word_meta(offset, word);
    }
    us1.enable_read();
    let end = timer.ticks_since(start);
    // Experimentally: 42.5kticks
    defmt::info!("Write took {=u32} ticks", end);

    defmt::info!("Starting partial erase...");
    let erase_start = timer.get_ticks();
    let mut min_time = 0xFFFF_FFFFu32;
    let mut max_time = 0x0000_0000u32;
    defmt::unwrap!(us1.start_partial_erase(1, SECTION_1_START_METADATA));

    loop {
        let start = timer.get_ticks();
        let res = us1.step_partial_erase();
        let elapsed = timer.ticks_since(start);
        min_time = min_time.min(elapsed);
        max_time = max_time.max(elapsed);


        match res {
            Ok(PartialStatus::Done) => break,
            Ok(PartialStatus::RemainingMs(_n)) => {
                while timer.millis_since(start) <= 10 { }
            },
            Err(_) => defmt::panic!(),
        }
    }

    defmt::info!("Erase complete! {=u32}", timer.ticks_since(erase_start));
    defmt::info!("Max: {=u32}, Min: {=u32}", max_time, min_time);

    let mut all_ffs = true;
    let meta_slice = unsafe {
        core::slice::from_raw_parts(
            SECTION_1_START_METADATA as *const u8,
            PAGE_SIZE,
        )
    };

    for ch in meta_slice.chunks_exact(16) {
        all_ffs &= ch.iter().all(|b| *b == 0xFF);
    }

    let end = timer.ticks_since(start);
    defmt::info!("All effs: {=bool}", all_ffs);

    anachro_boot::exit()
}
