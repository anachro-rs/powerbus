#![no_main]
#![no_std]

#![allow(unused_imports)]

use core::ptr::NonNull;

use anachro_boot::{self as _, bootload::{Metadata, Nvmc, PartialStatus, UsableSections}, consts::{PAGE_SIZE, SECTION_1_START_APP, SECTION_1_START_METADATA}}; // global logger + panicking-behavior + memory layout
use nrf52840_hal::{gpio::{Level, p0::Parts}, pac::Peripherals};
use nrf52840_hal::prelude::*;
use groundhog::RollingTimer;
use groundhog_nrf52::GlobalRollingTimer;

#[cortex_m_rt::entry]
fn main() -> ! {
    defmt::info!("Hello, world!");
    let board = defmt::unwrap!(Peripherals::take());
    let timer = GlobalRollingTimer::default();

    GlobalRollingTimer::init(board.TIMER0);

    let p0 = Parts::new(board.P0);
    let mut led_1 = p0.p0_13.into_push_pull_output(Level::High);
    let mut led_2 = p0.p0_14.into_push_pull_output(Level::Low);
    let _led_3 = p0.p0_15.into_push_pull_output(Level::High);
    let _led_4 = p0.p0_16.into_push_pull_output(Level::High);

    loop {
        let start = timer.get_ticks();
        led_1.set_low().ok();
        led_2.set_high().ok();
        while timer.millis_since(start) < 250 { }
        led_1.set_high().ok();
        led_2.set_low().ok();
        while timer.millis_since(start) < 1000 { }
    }
}
