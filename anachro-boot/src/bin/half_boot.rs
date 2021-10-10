#![no_main]
#![no_std]

#![allow(unused_imports)]

use core::ptr::NonNull;
use core::fmt::Write;

use anachro_boot::bootload::BootDecision;
use anachro_boot::{self as _, bootdata::Bootdata, bootload::{Metadata, Nvmc, PartialStatus, UsableSections, make_decision}, consts::{PAGE_SIZE, SECTION_1_START_APP, SECTION_1_START_METADATA}}; use heapless::String;
// global logger + panicking-behavior + memory layout
use nrf52840_hal::pac::{NVMC, Peripherals};
use groundhog::RollingTimer;
use groundhog_nrf52::GlobalRollingTimer;

static BOOT_1_IMG: &[u8] = include_bytes!("../../test-images/blink_1_app_combo.bin");
static BOOT_2_IMG: &[u8] = include_bytes!("../../test-images/blink_2_app_combo.bin");

#[cortex_m_rt::entry]
fn main() -> ! {
    defmt::info!("Hello, world!");
    let board = defmt::unwrap!(Peripherals::take());
    let timer = GlobalRollingTimer::default();

    GlobalRollingTimer::init(board.TIMER0);

    let nvmc = board.NVMC;

    let sec_meta_2 = if let Some(meta) = Metadata::from_section(UsableSections::Section2) {
        meta
    } else {
        fakeload_section(&nvmc, UsableSections::Section2, BOOT_1_IMG);
        defmt::unwrap!(Metadata::from_section(UsableSections::Section2))
    };

    let sec_meta_3 = if let Some(meta) = Metadata::from_section(UsableSections::Section3) {
        meta
    } else {
        fakeload_section(&nvmc, UsableSections::Section3, BOOT_2_IMG);
        defmt::unwrap!(Metadata::from_section(UsableSections::Section3))
    };

    defmt::info!("2 - uuid:  {:?}", sec_meta_2.image_uuid);
    defmt::info!("2 - pages: {:?}", sec_meta_2.image_len_pages);
    defmt::info!("2 - poly:  {:?}", sec_meta_2.image_poly1305_tag);

    defmt::info!("3 - uuid:  {:?}", sec_meta_3.image_uuid);
    defmt::info!("3 - pages: {:?}", sec_meta_3.image_len_pages);
    defmt::info!("3 - poly:  {:?}", sec_meta_3.image_poly1305_tag);

    let sec_meta_1 = if let Some(sec_meta_1) = Metadata::from_section(UsableSections::Section1) {
        defmt::info!("1 - uuid:  {:?}", sec_meta_1.image_uuid);
        defmt::info!("1 - pages: {:?}", sec_meta_1.image_len_pages);
        defmt::info!("1 - poly:  {:?}", sec_meta_1.image_poly1305_tag);
        Some(sec_meta_1)
    } else {
        defmt::info!("Sec 1 bad");
        None
    };

    let decision = make_decision(
        sec_meta_1,
        Some(sec_meta_2),
        Some(sec_meta_3),
    );

    // Grr...
    let mut buf: String<1024> = String::new();
    write!(&mut buf, "{:02X?}", decision).ok();
    defmt::info!("{:?}", buf.as_str());


    let bd = match decision {
        BootDecision::Boot(bd) => {
            bd
        },
        BootDecision::CopyThenFirstBoot { source, boot_seq, boot_dat } => {
            erase_app_section(&nvmc, source.image_len_pages);
            copy_section_to_app(&nvmc, source, boot_seq);
            boot_dat
        },
        BootDecision::RollbackThenBoot { source, boot_seq, boot_dat } => {
            erase_app_section(&nvmc, source.image_len_pages);
            copy_section_to_app(&nvmc, source, boot_seq);
            boot_dat
        },
        BootDecision::Halt => loop {
            cortex_m::asm::nop()
        }
    };

    let start = timer.get_ticks();
    defmt::info!("Booting in 1s!");

    while timer.millis_since(start) < 1000 { }

    defmt::unwrap!(bd.write_to(0x2003FC00));
    unsafe {
        cortex_m::asm::bootload(UsableSections::Section1.app_as_ptr() as *const u32);
    }

    // #[cfg(NOPE)]
    // {
    //     defmt::info!("Writing bootdata...");

    //     let bd = Bootdata {
    //         app_metadata: defmt::unwrap!(NonNull::new(sect.metadata_as_ptr())),
    //         own_metadata: defmt::unwrap!(NonNull::new(UsableSections::Section2.metadata_as_ptr())),
    //         nxt_metadata: defmt::unwrap!(NonNull::new(UsableSections::Section3.metadata_as_ptr())),
    //         nxt_image: defmt::unwrap!(NonNull::new(UsableSections::Section2.app_as_ptr())),
    //         is_first_boot: true,
    //         is_rollback: false,
    //     };

    //     defmt::unwrap!(bd.write_to(0x2003FC00));

    //     unsafe {
    //         cortex_m::asm::bootload(UsableSections::Section1.app_as_ptr() as *const u32);
    //     }
    // }
}

fn erase_app_section(nvmc: &NVMC, page_ct: usize) {
    // account for metadata page
    let page_ct = page_ct + 1;
    let base_ptr_usize = UsableSections::Section1.metadata_as_ptr() as usize;
    let mut us1 = Nvmc::new(nvmc, UsableSections::Section1);
    let timer = GlobalRollingTimer::default();

    for i_pg in 0..page_ct {
        defmt::info!("Starting erase of page {=usize}", i_pg);
        defmt::unwrap!(
            us1.start_partial_erase(
                1,
                base_ptr_usize + (i_pg * PAGE_SIZE)
            )
        );

        loop {
            let start = timer.get_ticks();
            match us1.step_partial_erase() {
                Ok(PartialStatus::Done) => break,
                Ok(PartialStatus::RemainingMs(_)) => {
                    // 50% duty cycle to allow for rtt
                    while timer.millis_since(start) < 2 { }
                }
                Err(_) => defmt::todo!(),
            }
        }

        defmt::info!("Finished erase.");
    }
}

fn copy_section_to_app(nvmc: &NVMC, source: Metadata, boot_seq: u32) {
    let app_ptr_usize = UsableSections::Section1.app_as_ptr() as usize;
    let mut us1 = Nvmc::new(nvmc, UsableSections::Section1);
    let timer = GlobalRollingTimer::default();
    let word_ct = source.image_len_pages * (PAGE_SIZE / 4);

    //
    // First, write application data
    //
    let page = unsafe {
        core::slice::from_raw_parts(
            source.section.app_as_ptr().cast::<u32>(),
            word_ct
        )
    };

    defmt::info!("Copying app...");

    let mut write_start = timer.get_ticks();
    us1.enable_write();
    for (i_wd, word) in page.iter().enumerate() {
        us1.write_word(app_ptr_usize + (4 * i_wd), *word);

        // 50% duty cycle for defmt
        if timer.ticks_since(write_start) > 1_000 {
            us1.enable_read();
            while timer.ticks_since(write_start) < 2_000 { }
            us1.enable_write();
            write_start = timer.get_ticks();
        }
    }
    us1.enable_read();

    defmt::info!("Copying header...");
    //
    // Then, write the header data
    //
    source.write_to_section(nvmc, UsableSections::Section1, boot_seq);
}

fn fakeload_section(nvmc: &NVMC, sect: UsableSections, img: &[u8]) {
    let mut us1 = Nvmc::new(nvmc, sect.clone());
    let timer = GlobalRollingTimer::default();

    let start = timer.get_ticks();
    defmt::info!("Pause...");
    while timer.millis_since(start) < 500 { }
    defmt::info!("Begin!...");

    assert_eq!(img.as_ptr() as usize & 0b11, 0);
    assert_eq!(img.len() & (PAGE_SIZE - 1), 0);

    let base_ptr_usize = sect.metadata_as_ptr() as usize;

    for (i_pg, page) in img.chunks_exact(PAGE_SIZE).enumerate() {
        defmt::info!("Starting erase of page {=usize}", i_pg);
        defmt::unwrap!(
            us1.start_partial_erase(
                1,
                base_ptr_usize + (i_pg * PAGE_SIZE)
            )
        );

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

            let page = base_ptr_usize + (i_pg * PAGE_SIZE);
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
}
