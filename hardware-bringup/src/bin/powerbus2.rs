#![no_main]
#![no_std]

use {
    embedded_hal::blocking::delay::DelayMs,
    groundhog_nrf52::GlobalRollingTimer,
    hardware_bringup as _, // global logger + panicking-behavior + memory layout
    nrf52840_hal::{
        self as hal,
        gpio::{p0::Parts as P0Parts, p1::Parts as P1Parts, Level},
        Rng, Timer,
    },
    nrf_smartled::pwm::Pwm,
    smart_leds::{colors, gamma, RGB8},
    smart_leds_trait::SmartLedsWrite,
};

use choreographer::{
    engine::{LoopBehavior, Sequence},
    script,
};

use heapless::{Vec, LinearMap};
use rand::prelude::SliceRandom;

const NUM_LEDS: usize = 100;

#[cortex_m_rt::entry]
fn main() -> ! {
    let board = hal::pac::Peripherals::take().unwrap();

    let mut timer = Timer::new(board.TIMER0);
    let p0 = P0Parts::new(board.P0);
    let _p1 = P1Parts::new(board.P1);

    GlobalRollingTimer::init(board.TIMER1);

    defmt::info!("Hello, world!");

    let smartled_data = p0.p0_13.into_push_pull_output(Level::Low);

    let mut leds = Pwm::new(board.PWM0, smartled_data.degrade());

    let mut rng = Rng::new(board.RNG);
    let mut pixels = [colors::BLACK; NUM_LEDS];
    let mut script: [Sequence<GlobalRollingTimer, 8>; NUM_LEDS] = Sequence::new_array();
    let mut idle: LinearMap<usize, (), NUM_LEDS> = LinearMap::new();

    for i in 0..NUM_LEDS {
        defmt::unwrap!(idle.insert(i, ()));
    }

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

    let mut color_iter = color_path.iter().cycle();
    let mut active_ct: u8 = 10;
    let mut color_ct: u16 = 0;
    let mut target_delay: u32 = 1_000;
    let mut current_delay: u32 = 1_000;
    let mut color: RGB8 = colors::BLACK;

    loop {
        if active_ct == 0 {
            // active_ct = (rng.random_u8() % 16) + 4;
            // defmt::info!("New color! Next: {=u8}", active_ct);
            let mut idle_choices: Vec<usize, NUM_LEDS> = Vec::new();
            idle.keys().for_each(|k| {
                idle_choices.push(*k).ok();
            });
            let new_idx = idle_choices.choose(&mut rng).map(|k| {
                idle.remove(k);
                *k
            });

            if let Some(num) = new_idx {
                let dur = (rng.random_u32() % 4000) + 1000;
                let dur_f = dur as f32;

                script[num].set(&script! {
                    | action | (color) |    (duration_ms) | (period_ms_f) | (phase_offset_ms) | repeat |
                    |  solid | (BLACK) | ( current_delay) |      (   0.0) |               (0) |   once |
                    |    sin | (color) | (           dur) |      ( dur_f) |               (0) |   once |
                    |  solid | (BLACK) | ( current_delay) |      (   0.0) |               (0) |   once |
                }, LoopBehavior::OneShot);

                if color_ct == 0 {
                    color = if (rng.random_u8() & 0x1) == 0 {
                        defmt::info!("RAINBOW");
                        *color_iter.next().unwrap()
                    } else {
                        defmt::info!("RANDOM");
                        loop {
                            let new_color = RGB8 {
                                r: rng.random_u8(),
                                g: rng.random_u8(),
                                b: rng.random_u8(),
                            };

                            if (new_color.r as u16 + new_color.g as u16 + new_color.b as u16) >= 255 {
                                break new_color;
                            }
                        }

                    };

                    color_ct = (rng.random_u16() % (4 * NUM_LEDS) as u16).min(NUM_LEDS as u16 / 2);
                } else {
                    color_ct -= 1;
                }
            } else {
                defmt::info!("SKIP!");
            }
            active_ct = (rng.random_u8() / 2).min(10);
        } else {
            active_ct = active_ct.saturating_sub(1);
        }

        if target_delay == current_delay {
            target_delay = (rng.random_u32() % 10_000).min(10);
        } else if current_delay < target_delay {
            current_delay += 1;
        } else {
            current_delay -= 1;
        }

        for (i, (pix, scr)) in pixels.iter_mut().zip(script.iter_mut()).enumerate() {
            if let Some(newval) = scr.poll() {
                *pix = newval;
                // todo
                core::mem::swap(&mut pix.r, &mut pix.g);
            } else {
                defmt::unwrap!(idle.insert(i, ()));
            }
        }

        leds.write(gamma(pixels.iter().cloned())).ok();
        timer.delay_ms(5u32);
    }
}
