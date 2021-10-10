#![no_std]

use core::sync::atomic::{AtomicUsize, Ordering};

use defmt_rtt as _; // global logger
// TODO(5) adjust HAL import
// use some_hal as _; // memory layout

use groundhog::RollingTimer;
use groundhog_nrf52::GlobalRollingTimer;
use panic_probe as _;

pub mod consts;
pub mod bootload;
pub mod bootdata;

// same panicking *behavior* as `panic-probe` but doesn't print a panic message
// this prevents the panic message being printed *twice* when `defmt::panic` is invoked
#[defmt::panic_handler]
fn panic() -> ! {
    cortex_m::asm::udf()
}

defmt::timestamp!("{=u32:010}", {
    let timer = GlobalRollingTimer::default();
    timer.get_ticks()
});

/// Terminates the application and makes `probe-run` exit with exit-code = 0
pub fn exit() -> ! {
    loop {
        cortex_m::asm::bkpt();
    }
}
