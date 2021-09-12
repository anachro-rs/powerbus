#![no_main]
#![no_std]

use {
    embedded_hal::blocking::delay::DelayMs,
    nrf52840_hal::{
        self as hal,
        gpio::{p0::Parts as P0Parts, p1::Parts as P1Parts, Level},
        Rng, Timer,
    },
    hardware_bringup as _, // global logger + panicking-behavior + memory layout
    nrf_smartled::pwm::Pwm,
    smart_leds::{colors, gamma, RGB8},
    smart_leds_trait::SmartLedsWrite,
};

const NUM_LEDS: usize = 10;

#[cortex_m_rt::entry]
fn main() -> ! {
    defmt::info!("Hello, world!");

    let board = hal::pac::Peripherals::take().unwrap();
    // Make sure the voltage is not ever reset to 1.8V!

    let mut timer = Timer::new(board.TIMER0);
    let p0 = P0Parts::new(board.P0);
    let _p1 = P1Parts::new(board.P1);

    // P0_13<Disconnected>

    let smartled_data = p0.p0_13.into_push_pull_output(Level::Low);

    let mut leds = Pwm::new(board.PWM0, smartled_data.degrade());

    let _rng = Rng::new(board.RNG);
    let mut pixels = [RGB8::default(); NUM_LEDS];
    let mut base_pixels = [RGB8::default(); NUM_LEDS];

    leds.write(pixels.iter().cloned()).ok();

    let color_path = &[
        colors::WHITE,
        colors::RED,
        colors::ORANGE,
        colors::YELLOW,
        colors::GREEN,
        colors::BLUE,
        colors::INDIGO,
        colors::VIOLET,
    ];

    let mut ct: u8 = 0;

    let mut color_iter = color_path.iter().cycle();

    let mut num: u8 = 0;
    pixels.iter_mut().for_each(|pixel| {
        pixel.r = pixel.r + num;
        num = num.wrapping_add(10);
    });

    let mut active_ct = 0u32;

    loop {
        if (ct == 0) || (ct == 128) {
            defmt::info!("New colors!");
            let col = color_iter.next().unwrap();
            for pix in base_pixels.iter_mut() {
                *pix = *col;
            }
            // vibe.set_high().ok();
            active_ct = 10;
        }

        active_ct = active_ct.saturating_sub(1);
        if active_ct == 0 {
            // vibe.set_low().ok();
        }

        for (pixel, base) in pixels.iter_mut().zip(base_pixels.iter()) {
            pixel.r = libm::fabsf(
                (base.r as f32) * (libm::sinf((ct as f32 / 255.0) * core::f32::consts::PI * 2.0)),
            ) as u8;
            pixel.g = libm::fabsf(
                (base.g as f32) * (libm::sinf((ct as f32 / 255.0) * core::f32::consts::PI * 2.0)),
            ) as u8;
            pixel.b = libm::fabsf(
                (base.b as f32) * (libm::sinf((ct as f32 / 255.0) * core::f32::consts::PI * 2.0)),
            ) as u8;

            // HACK for fairy lights
            // core::mem::swap(&mut pixel.r, &mut pixel.g);
        }

        ct = ct.wrapping_add(1);

        leds.write(
            gamma(
                pixels.iter().cloned()
            )
        ).ok();
        timer.delay_ms(10u32);
    }
}
