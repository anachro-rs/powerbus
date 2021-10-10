#![no_main]
#![no_std]

use anachro_boot as _; // global logger + panicking-behavior + memory layout

#[cortex_m_rt::entry]
fn main() -> ! {
    defmt::info!("Hello, world!");

    anachro_boot::exit()
}
