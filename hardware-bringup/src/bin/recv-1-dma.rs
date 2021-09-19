#![no_main]
#![no_std]

use hardware_bringup::{self as _, PowerBusPins};
use nrf52840_hal::{
    gpio::Level,
    pac::{Interrupt, Peripherals, TIMER2, UARTE0},
    ppi::{Parts as PpiParts, Ppi3},
    prelude::OutputPin,
    uarte::{Baudrate, Parity, Pins as UartePins, Uarte},
    Timer,
};
// use groundhog::RollingTimer;
use anachro_485::dispatch::{IoHandle, IoQueue};
use anachro_485::icd::{SLAB_SIZE, TOTAL_SLABS};
use byte_slab::BSlab;
use groundhog_nrf52::GlobalRollingTimer;
use uarte_485::{Pin485, Uarte485};

static IOQ: IoQueue = IoQueue::new();
static BSLAB: BSlab<TOTAL_SLABS, SLAB_SIZE> = BSlab::new();

#[rtic::app(device = nrf52840_hal::pac, peripherals = true, monotonic = groundhog_nrf52::GlobalRollingTimer)]
const APP: () = {
    struct Resources {
        usart: Uarte485<TIMER2, Ppi3, UARTE0>,
    }

    #[init]
    fn init(cx: init::Context) -> init::LateResources {
        defmt::info!("Hello, world!");
        defmt::info!("Receiving on Port 1 (Bus)");
        BSLAB.init().unwrap();

        let board = cx.device;

        GlobalRollingTimer::init(board.TIMER0);
        let _timer = GlobalRollingTimer::default();
        let _timer_2 = Timer::new(board.TIMER1);

        let pins = PowerBusPins::from_ports(board.P0, board.P1);

        let _led1 = pins.led_1.into_push_pull_output(Level::High);
        let _led2 = pins.led_2.into_push_pull_output(Level::High);
        let _ = pins.rs2_de.into_push_pull_output(Level::Low);      // Disabled
        let _ = pins.rs2_re_n.into_push_pull_output(Level::High);   // Disabled
        let ppi = PpiParts::new(board.PPI);

        let uarrr = Uarte485::new(
            &BSLAB,
            board.TIMER2,
            ppi.ppi3,
            board.UARTE0,
            Pin485 {
                rs_di: pins.rs1_di.degrade(),
                rs_ro: pins.rs1_ro.degrade(),
                rs_de: pins.rs1_de.degrade(),
                rs_re_n: pins.rs1_re_n.degrade(),
            },
            IOQ.take_io_handle().unwrap(),
        );

        init::LateResources { usart: uarrr }

        // let mut serial = Uarte::new(
        //     board.UARTE0,
        //     UartePins {
        //         rxd: pins.rs1_ro.into_floating_input().degrade(),
        //         txd: pins.rs1_di.into_push_pull_output(Level::Low).degrade(),
        //         cts: None,
        //         rts: None,
        //     },
        //     Parity::EXCLUDED,
        //     Baudrate::BAUD1M,
        // );

        // let mut buf = [0u8; 255];

        // let _ = pins.rs1_de.into_push_pull_output(Level::Low);      // Disabled
        // let _ = pins.rs1_re_n.into_push_pull_output(Level::Low);    // Enabled
        // let _ = pins.rs2_de.into_push_pull_output(Level::Low);      // Disabled
        // let _ = pins.rs2_re_n.into_push_pull_output(Level::High);   // Disabled

        // loop {
        //     led1.set_low().ok();
        //     led2.set_high().ok();

        //     match serial.read_timeout(&mut buf[..5], &mut timer_2, 64_000_000) {
        //         Ok(_) => {
        //             let strng = defmt::unwrap!(core::str::from_utf8(&buf[..5]).map_err(drop));
        //             defmt::info!("Got: {:?}", strng);
        //         },
        //         Err(_) => {
        //             defmt::warn!("Timeout :(");
        //         },
        //     }

        //     led1.set_high().ok();
        //     led2.set_low().ok();

        //     match serial.read_timeout(&mut buf[..5], &mut timer_2, 64_000_000) {
        //         Ok(_) => {
        //             let strng = defmt::unwrap!(core::str::from_utf8(&buf[..5]).map_err(drop));
        //             defmt::info!("Got: {:?}", strng);
        //         },
        //         Err(_) => {
        //             defmt::warn!("Timeout :(");
        //         },
        //     }
        // }
    }

    #[idle]
    fn idle(ctx: idle::Context) -> ! {
        IOQ.io_recv_auth.store(true, core::sync::atomic::Ordering::SeqCst);
        rtic::pend(Interrupt::UARTE0_UART0);

        loop {
            if let Some(msg) = IOQ.to_dispatch.dequeue() {
                let strng = defmt::unwrap!(core::str::from_utf8(&msg.packet[..msg.len]).map_err(drop));
                defmt::info!("Got: {:?}", strng);
            }
        }
    }

    #[task(binds = UARTE0_UART0, resources = [usart])]
    fn uarte(ctx: uarte::Context) {
        // defmt::warn!("INT: uarte");
        ctx.resources.usart.uarte_interrupt();
    }

    #[task(binds = TIMER2)]
    fn timer(ctx: timer::Context) {
        // TODO: It looks like we might have a spurious timer interrupt?
        // defmt::warn!("INT: timer");
        rtic::pend(Interrupt::UARTE0_UART0);
    }
};
