#![no_main]
#![no_std]

use core::iter::Chain;

use groundhog::RollingTimer;
use groundhog_nrf52::GlobalRollingTimer;
use hardware_bringup as _; // global logger + panicking-behavior + memory layout
use nrf52840_hal::{pac::Peripherals, rng::Rng};
use rand::{Rng as _, SeedableRng};
use rand_chacha::ChaCha8Rng;

#[cortex_m_rt::entry]
fn main() -> ! {
    defmt::info!("Hello, world!");

    let board = Peripherals::take().unwrap();
    GlobalRollingTimer::init(board.TIMER0);
    let timer = GlobalRollingTimer::new();

    let t0 = timer.get_ticks();
    let mut rng = Rng::new(board.RNG);
    let t1 = timer.ticks_since(t0);
    defmt::info!("Init time: {=u32}", t1);

    let mut seed = [0u8; 32];
    seed.iter_mut().for_each(|t| *t = rng.random_u8());
    let mut png = ChaCha8Rng::from_seed(seed);

    loop {
        let mut buf = [0u32; 10];
        let t2 = timer.get_ticks();
        buf.iter_mut()
            .for_each(|t| *t = rng.gen_range(10_000..100_000));
        let t3 = timer.ticks_since(t2);
        defmt::info!("Gen time (ttl): {=u32}", t3);
        defmt::info!("Gen time (avg): {=u32}", t3 / buf.len() as u32);
        defmt::info!("Generated: {=?}", buf);

        while timer.millis_since(t2) <= 500 {}

        let mut buf = [0u32; 10];
        let t4 = timer.get_ticks();
        buf.iter_mut()
            .for_each(|t| *t = png.gen_range(10_000..100_000));
        let t5 = timer.ticks_since(t4);
        defmt::info!("Gen time (ttl): {=u32}", t5);
        defmt::info!("Gen time (avg): {=u32}", t5 / buf.len() as u32);
        defmt::info!("Generated: {=?}", buf);

        while timer.millis_since(t2) <= 500 {}
    }
}
