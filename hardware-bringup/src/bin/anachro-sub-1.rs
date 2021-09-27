#![no_main]
#![no_std]

use hardware_bringup::{self as _, PowerBusPins};
use nrf52840_hal::{Timer, gpio::{Level, Output, Pin, PushPull}, pac::{Interrupt, TIMER2, UARTE0}, ppi::{Parts as PpiParts, Ppi3}, rng::Rng, prelude::OutputPin};
// use groundhog::RollingTimer;
use anachro_485::{dispatch::{IoQueue, Dispatch}, dom::MANAGEMENT_PORT};
use anachro_485::icd::{SLAB_SIZE, TOTAL_SLABS};
use byte_slab::BSlab;
use groundhog_nrf52::GlobalRollingTimer;
use uarte_485::{Pin485, Uarte485, DefaultTo};

use anachro_485::sub::discover::Discovery;
use cassette::{pin_mut, Cassette};

static IOQ: IoQueue = IoQueue::new();
static BSLAB: BSlab<TOTAL_SLABS, SLAB_SIZE> = BSlab::new();
static DISPATCH: Dispatch<8> = Dispatch::new(&IOQ, &BSLAB);

#[rtic::app(device = nrf52840_hal::pac, peripherals = true, monotonic = groundhog_nrf52::GlobalRollingTimer)]
const APP: () = {
    struct Resources {
        usart: Uarte485<TIMER2, Ppi3, UARTE0, GlobalRollingTimer>,
        opt_rng: Option<Rng>,
        led1: Pin<Output<PushPull>>,
        led2: Pin<Output<PushPull>>,
    }

    #[init]
    fn init(cx: init::Context) -> init::LateResources {
        defmt::info!("Hello, world!");
        defmt::info!("Sub on Port 1 (Bus)");
        BSLAB.init().unwrap();

        let board = cx.device;

        GlobalRollingTimer::init(board.TIMER0);
        let _timer = GlobalRollingTimer::default();
        let _timer_2 = Timer::new(board.TIMER1);

        let pins = PowerBusPins::from_ports(board.P0, board.P1);

        let led1 = pins.led_1.into_push_pull_output(Level::High).degrade();
        let led2 = pins.led_2.into_push_pull_output(Level::High).degrade();
        let _ = pins.rs2_de.into_push_pull_output(Level::Low);      // Disabled
        let _ = pins.rs2_re_n.into_push_pull_output(Level::High);   // Disabled
        let ppi = PpiParts::new(board.PPI);

        let rand = Rng::new(board.RNG);

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
            DefaultTo::Receiving,
        );

        init::LateResources { usart: uarrr, opt_rng: Some(rand), led1, led2 }
    }

    #[idle(resources = [opt_rng, led1, led2])]
    fn idle(ctx: idle::Context) -> ! {
        rtic::pend(Interrupt::UARTE0_UART0);

        let rand = ctx.resources.opt_rng.take().unwrap();

        let mgmt_socket = DISPATCH
            .register_port(MANAGEMENT_PORT).unwrap();

        let mut sub_disco: Discovery<GlobalRollingTimer, _> =
            Discovery::new(rand, &DISPATCH, mgmt_socket, &BSLAB);
        let sub_disco_future = sub_disco.obtain_addr();
        pin_mut!(sub_disco_future);

        let mut cas_sub_disco = Cassette::new(sub_disco_future);

        let mut addr_oneshot = false;

        loop {
            // TODO: PROOOOOBABLY need to do tx/rx auth stuff

            // Process messages
            DISPATCH.process_messages();

            // Check the actual tasks
            if let Some(msg) = cas_sub_disco.poll_on() {
                if let Ok(addr) = msg {
                    defmt::info!("got address! {=u8}", addr);
                } else {
                    defmt::error!("WAT?!?");
                }

                hardware_bringup::exit();
            }

            if !addr_oneshot {
                if let Some(addr) = DISPATCH.get_addr() {
                    addr_oneshot = true;
                    defmt::info!("Got address: {=u8}", addr);
                    ctx.resources.led1.set_low().ok();
                    ctx.resources.led2.set_low().ok();
                } else {
                    addr_oneshot = false;
                    ctx.resources.led1.set_high().ok();
                    ctx.resources.led2.set_high().ok();
                }
            }
        }
    }

    #[task(binds = UARTE0_UART0, resources = [usart])]
    fn uarte(ctx: uarte::Context) {
        // defmt::warn!("INT: uarte");
        ctx.resources.usart.uarte_interrupt();
    }

    #[task(binds = TIMER2, resources = [usart])]
    fn timer(ctx: timer::Context) {
        // TODO: It looks like we might have a spurious timer interrupt?
        // defmt::warn!("INT: timer");
        ctx.resources.usart.timer_interrupt();
    }
};
