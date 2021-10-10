#![no_main]
#![no_std]

#![allow(unused_imports)]

use core::ptr::NonNull;

use anachro_boot::{self as _, bootload::{Metadata, Nvmc, UsableSections}, consts::{PAGE_SIZE, SECTION_1_START_APP, SECTION_1_START_METADATA}}; // global logger + panicking-behavior + memory layout
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

    let mut all_ffs = true;

    defmt::info!("Starting read...");
    let start = timer.get_ticks();

    // Create an unsafe slice to access the flash memory.
    // Scope is used to drop the slice before we start poking
    // around with the flash controller
    {
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
        // Experimentally: 833 ticks
        defmt::info!("Read took {=u32} ticks", end);
        defmt::info!("All effs: {=bool}", all_ffs);
    }

    // This seems to always be true. Likely, the debugger is
    // wiping the entire memory region, rather than just the
    // necessary parts.
    if !all_ffs {
        defmt::info!("Not empty! Erasing metadata...");
        us1.enable_erase();
        us1.erase_metadata();
        us1.enable_read();

        let mut all_ffs = true;

        {
            let meta_slice = unsafe {
                core::slice::from_raw_parts(
                    SECTION_1_START_METADATA as *const u8,
                    PAGE_SIZE,
                )
            };

            for ch in meta_slice.chunks_exact(16) {
                all_ffs &= ch.iter().all(|b| *b == 0xFF);
            }

            defmt::info!("All effs: {=bool}", all_ffs);
        }
    }

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

    let mut all_ffs = true;

    {
        let meta_slice = unsafe {
            core::slice::from_raw_parts(
                SECTION_1_START_METADATA as *const u8,
                PAGE_SIZE,
            )
        };

        for ch in meta_slice.chunks_exact(16) {
            all_ffs &= ch.iter().all(|b| *b == 0xFF);
        }

        defmt::info!("All effs: {=bool}", all_ffs);
    }

    let mut meta = defmt::unwrap!(Metadata::from_section(UsableSections::Section1));

    defmt::info!("image_uuid: {:?}", meta.image_uuid);
    defmt::info!("image_poly1305_tag: {:?}", meta.image_poly1305_tag);
    defmt::info!("image_len_pages: {:?}", meta.image_len_pages);
    defmt::info!("boot_seq_number: {:?}", meta.boot_seq_number);
    defmt::info!("flashed_tagword: {:?}", meta.flashed_tagword);
    defmt::info!("booted_tagword: {:?}", meta.booted_tagword);
    defmt::info!("app_ptr: {:?}", meta.app_ptr.as_ptr() as usize);

    defmt::info!("Tweaking...");
    meta.image_len_pages = 63;
    meta.app_ptr = defmt::unwrap!(NonNull::new(SECTION_1_START_APP as *const u8 as *mut u8));

    defmt::info!("Generating new signature...");
    let start = timer.get_ticks();
    let new_sig = defmt::unwrap!(meta.generate_poly_tag());
    let end = timer.ticks_since(start);

    // Experimentally: 50kticks (63 pages)
    // This means 194ns/byte
    // 256 bytes: 49.6us
    // 4Kibytes:  794us
    defmt::info!("Signature took {=u32} ticks", end);
    defmt::info!("New sig: {:?}", new_sig);

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

    anachro_boot::exit()
}
