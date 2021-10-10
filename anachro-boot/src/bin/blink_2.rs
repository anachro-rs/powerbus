#![no_main]
#![no_std]
#![allow(unused_imports)]

use core::ptr::NonNull;
use core::fmt::Write;

use anachro_boot::{
    self as _,
    bootload::{Metadata, Nvmc, PartialStatus, UsableSections},
    consts::{PAGE_SIZE, SECTION_1_START_APP, SECTION_1_START_METADATA},
    bootdata::Bootdata,
}; // global logger + panicking-behavior + memory layout
use groundhog::RollingTimer;
use groundhog_nrf52::GlobalRollingTimer;
use nrf52840_hal::prelude::*;
use nrf52840_hal::{
    gpio::{p0::Parts, Level},
    pac::Peripherals,
    uarte::{Baudrate, Parity, Pins},
    Uarte,
};

use heapless::String;

#[cortex_m_rt::entry]
fn main() -> ! {
    defmt::info!("Hello, world!");
    let board = defmt::unwrap!(Peripherals::take());
    let timer = GlobalRollingTimer::default();

    GlobalRollingTimer::init(board.TIMER0);

    let p0 = Parts::new(board.P0);

    let mut uarte = Uarte::new(
        board.UARTE0,
        Pins {
            rxd: p0.p0_08.into_floating_input().degrade(),
            txd: p0.p0_06.into_push_pull_output(Level::Low).degrade(),
            cts: Some(p0.p0_07.into_floating_input().degrade()),
            rts: Some(p0.p0_05.into_push_pull_output(Level::Low).degrade()),
        },
        Parity::EXCLUDED,
        Baudrate::BAUD115200,
    );

    let mut led_1 = p0.p0_13.into_push_pull_output(Level::High);
    let mut led_2 = p0.p0_14.into_push_pull_output(Level::Low);
    let _led_3 = p0.p0_15.into_push_pull_output(Level::High);
    let _led_4 = p0.p0_16.into_push_pull_output(Level::High);

    let mut strbuf: String<1024> = String::new();
    write!(&mut strbuf, "Hello, world! - Blink 2\r\n").ok();
    defmt::unwrap!(uarte.write(strbuf.as_bytes()).map_err(drop));

    strbuf.clear();
    if let Some(bd) = Bootdata::load_from(0x2003FC00) {

        if let (Some(sto_meta), Some(app_meta)) = (Metadata::from_addr(bd.own_metadata), Metadata::from_addr(bd.app_metadata)) {
            sto_meta.mark_booted(&board.NVMC);
            app_meta.mark_booted(&board.NVMC);
        }

        write!(&mut strbuf, "Got boot data!\r\n{:#?}\r\n", bd).ok();
    } else {
        write!(&mut strbuf, "No boot data :(\r\n").ok();
    }
    defmt::unwrap!(uarte.write(strbuf.as_bytes()).map_err(drop));


    loop {
        strbuf.clear();
        let start = timer.get_ticks();
        led_1.set_low().ok();
        led_2.set_high().ok();
        write!(&mut strbuf, "Ding!\r\n").ok();
        defmt::unwrap!(uarte.write(strbuf.as_bytes()).map_err(drop));
        while timer.millis_since(start) < 250 { }
        led_1.set_high().ok();
        led_2.set_low().ok();
        while timer.millis_since(start) < 1000 { }
    }
}
