#![no_main]
#![no_std]

use hardware_bringup::{self as _, PowerBusPins};
use nrf52840_hal::{Timer, gpio::Level, pac::Peripherals, prelude::OutputPin, uarte::{Baudrate, Parity, Pins as UartePins, Uarte}};
// use groundhog::RollingTimer;
use groundhog_nrf52::GlobalRollingTimer;

#[cortex_m_rt::entry]
fn main() -> ! {
    defmt::info!("Hello, world!");
    defmt::info!("Receiving on Port 1 (Bus)");

    let board = defmt::unwrap!(Peripherals::take());

    GlobalRollingTimer::init(board.TIMER0);
    let _timer = GlobalRollingTimer::default();
    let mut timer_2 = Timer::new(board.TIMER1);

    let pins = PowerBusPins::from_ports(board.P0, board.P1);

    let mut led1 = pins.led_1.into_push_pull_output(Level::High);
    let mut led2 = pins.led_2.into_push_pull_output(Level::High);

    let mut serial = Uarte::new(
        board.UARTE0,
        UartePins {
            rxd: pins.rs1_ro.into_floating_input().degrade(),
            txd: pins.rs1_di.into_push_pull_output(Level::Low).degrade(),
            cts: None,
            rts: None,
        },
        Parity::EXCLUDED,
        Baudrate::BAUD1M,
    );

    let mut buf = [0u8; 255];

    let _ = pins.rs1_de.into_push_pull_output(Level::Low);      // Disabled
    let _ = pins.rs1_re_n.into_push_pull_output(Level::Low);    // Enabled
    let _ = pins.rs2_de.into_push_pull_output(Level::Low);      // Disabled
    let _ = pins.rs2_re_n.into_push_pull_output(Level::High);   // Disabled

    loop {
        led1.set_low().ok();
        led2.set_high().ok();


        match serial.read_timeout(&mut buf[..5], &mut timer_2, 64_000_000) {
            Ok(_) => {
                let strng = defmt::unwrap!(core::str::from_utf8(&buf[..5]).map_err(drop));
                defmt::info!("Got: {:?}", strng);
            },
            Err(_) => {
                defmt::warn!("Timeout :(");
            },
        }

        led1.set_high().ok();
        led2.set_low().ok();

        match serial.read_timeout(&mut buf[..5], &mut timer_2, 64_000_000) {
            Ok(_) => {
                let strng = defmt::unwrap!(core::str::from_utf8(&buf[..5]).map_err(drop));
                defmt::info!("Got: {:?}", strng);
            },
            Err(_) => {
                defmt::warn!("Timeout :(");
            },
        }
    }
}
