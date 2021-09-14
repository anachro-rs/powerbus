#![no_std]

use defmt_rtt as _; // global logger
                    // memory layout
use nrf52840_hal::{
    gpio::{
        p0::{self, Parts as P0Parts},
        p1::{self, Parts as P1Parts},
        Disconnected,
    },
    pac::{P0, P1},
};

use panic_probe as _;

use groundhog::RollingTimer;
use groundhog_nrf52::GlobalRollingTimer;

// same panicking *behavior* as `panic-probe` but doesn't print a panic message
// this prevents the panic message being printed *twice* when `defmt::panic` is invoked
#[defmt::panic_handler]
fn panic() -> ! {
    cortex_m::asm::udf()
}

defmt::timestamp!("{=usize}", {
    let timer = GlobalRollingTimer::default();
    timer.get_ticks() as usize
});

/// Terminates the application and makes `probe-run` exit with exit-code = 0
pub fn exit() -> ! {
    loop {
        cortex_m::asm::bkpt();
    }
}

pub struct PowerBusPins {
    // Auxillary I/Os - "spare"
    pub io_1: p0::P0_10<Disconnected>,
    pub io_2: p0::P0_09<Disconnected>,
    pub io_3: p1::P1_00<Disconnected>,
    pub io_4: p0::P0_02<Disconnected>,
    pub io_5: p1::P1_15<Disconnected>,
    pub io_6: p1::P1_13<Disconnected>,
    pub io_7: p0::P0_24<Disconnected>,

    pub led_1: p0::P0_18<Disconnected>,
    pub led_2: p0::P0_15<Disconnected>,

    // qwiic i2c (both 3v3 and 5v0 ports)
    pub i2c_scl: p0::P0_22<Disconnected>,
    pub i2c_sda: p0::P0_20<Disconnected>,

    // smartled out (to level shifter)
    pub smartled: p0::P0_13<Disconnected>,

    // RS-485 - 1
    pub rs1_di: p0::P0_29<Disconnected>,
    pub rs1_ro: p0::P0_31<Disconnected>,
    pub rs1_de: p0::P0_06<Disconnected>,
    pub rs1_re_n: p1::P1_09<Disconnected>,

    // RS-485 - 2
    pub rs2_di: p0::P0_26<Disconnected>,
    pub rs2_ro: p0::P0_04<Disconnected>,
    pub rs2_de: p0::P0_08<Disconnected>,
    pub rs2_re_n: p0::P0_12<Disconnected>,
}

impl PowerBusPins {
    pub fn from_ports(p0: P0, p1: P1) -> Self {
        let p0p = P0Parts::new(p0);
        let p1p = P1Parts::new(p1);

        Self {
            io_1: p0p.p0_10,
            io_2: p0p.p0_09,
            io_3: p1p.p1_00,
            io_4: p0p.p0_02,
            io_5: p1p.p1_15,
            io_6: p1p.p1_13,
            io_7: p0p.p0_24,

            led_1: p0p.p0_18,
            led_2: p0p.p0_15,

            i2c_scl: p0p.p0_22,
            i2c_sda: p0p.p0_20,

            smartled: p0p.p0_13,

            rs1_di: p0p.p0_29,
            rs1_ro: p0p.p0_31,
            rs1_de: p0p.p0_06,
            rs1_re_n: p1p.p1_09,

            rs2_di: p0p.p0_26,
            rs2_ro: p0p.p0_04,
            rs2_de: p0p.p0_08,
            rs2_re_n: p0p.p0_12,
        }
    }
}
