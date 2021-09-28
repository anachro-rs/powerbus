#![no_main]
#![no_std]

use groundhog::RollingTimer;
use groundhog_nrf52::GlobalRollingTimer;
use hardware_bringup::{self as _, PowerBusPins};
use nrf52840_hal::{
    gpio::Level,
    pac::Peripherals,
    prelude::OutputPin,
    uarte::{Baudrate, Parity, Pins as UartePins, Uarte},
};

#[cortex_m_rt::entry]
fn main() -> ! {
    defmt::info!("Hello, world!");

    let board = defmt::unwrap!(Peripherals::take());

    GlobalRollingTimer::init(board.TIMER0);
    let timer = GlobalRollingTimer::default();

    let pins = PowerBusPins::from_ports(board.P0, board.P1);

    let mut led1 = pins.led_1.into_push_pull_output(Level::High);
    let mut led2 = pins.led_2.into_push_pull_output(Level::High);

    let mut serial = Uarte::new(
        board.UARTE0,
        UartePins {
            rxd: pins.rs2_ro.into_floating_input().degrade(),
            txd: pins.rs2_di.into_push_pull_output(Level::Low).degrade(),
            cts: None,
            rts: None,
        },
        Parity::EXCLUDED,
        Baudrate::BAUD1M,
    );

    let mut buf = [0u8; 255];
    let ping = b"Ping!";
    let pong = b"Pong!";

    let _ = pins.rs1_de.into_push_pull_output(Level::Low); // Disabled
    let _ = pins.rs1_re_n.into_push_pull_output(Level::High); // Disabled
    let mut txmit = pins.rs2_de.into_push_pull_output(Level::Low); // Disabled
    let _ = pins.rs2_re_n.into_push_pull_output(Level::High); // Disabled

    loop {
        let start = timer.get_ticks();
        led1.set_low().ok();
        led2.set_high().ok();

        buf[..ping.len()].copy_from_slice(ping);

        defmt::info!("Send ping...");
        txmit.set_high().ok();
        let now = timer.get_ticks();
        while timer.micros_since(now) < 1 {}
        defmt::unwrap!(serial.write(&buf[..ping.len()]).map_err(drop));
        let now = timer.get_ticks();
        while timer.micros_since(now) < 1 {}
        txmit.set_low().ok();

        while timer.millis_since(start) <= 1000 {}

        buf[..pong.len()].copy_from_slice(pong);

        defmt::info!("Send pong...");
        txmit.set_high().ok();
        let now = timer.get_ticks();
        while timer.micros_since(now) < 1 {}
        defmt::unwrap!(serial.write(&buf[..pong.len()]).map_err(drop));
        let now = timer.get_ticks();
        while timer.micros_since(now) < 1 {}
        txmit.set_low().ok();

        led1.set_high().ok();
        led2.set_low().ok();

        while timer.millis_since(start) <= 2000 {}
    }
}
