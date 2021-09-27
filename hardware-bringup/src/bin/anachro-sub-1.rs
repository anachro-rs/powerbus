#![no_main]
#![no_std]

use groundhog::RollingTimer;
use hardware_bringup::{self as _, PowerBusPins};
use nrf52840_hal::{Timer, gpio::{Level, Output, Pin, PushPull}, pac::{Interrupt, SCB, TIMER2, UARTE0}, ppi::{Parts as PpiParts, Ppi3}, prelude::OutputPin, rng::Rng};
// use groundhog::RollingTimer;
use anachro_485::{dispatch::{IoQueue, Dispatch}, dom::{DISCOVERY_PORT, TOKEN_PORT}, sub::token::Token};
use anachro_485::icd::{SLAB_SIZE, TOTAL_SLABS};
use byte_slab::BSlab;
use groundhog_nrf52::GlobalRollingTimer;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
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
        opt_rng: Option<(ChaCha8Rng, ChaCha8Rng)>,
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
        let timer = GlobalRollingTimer::default();
        let _timer_2 = Timer::new(board.TIMER1);

        let pins = PowerBusPins::from_ports(board.P0, board.P1);

        let mut led1 = pins.led_1.into_push_pull_output(Level::High).degrade();
        let mut led2 = pins.led_2.into_push_pull_output(Level::High).degrade();
        let _ = pins.rs2_de.into_push_pull_output(Level::Low);      // Disabled
        let _ = pins.rs2_re_n.into_push_pull_output(Level::High);   // Disabled
        let ppi = PpiParts::new(board.PPI);

        for _ in 0..3 {
            let start = timer.get_ticks();
            led1.set_low().ok();
            led2.set_low().ok();
            while timer.millis_since(start) <= 250 { }
            led1.set_high().ok();
            led2.set_high().ok();
            while timer.millis_since(start) <= 1000 { }
        }

        let mut rand = Rng::new(board.RNG);

        let mut seed_1 = [0u8; 32];
        seed_1.iter_mut().for_each(|t| *t = rand.random_u8());
        let rand_1 = ChaCha8Rng::from_seed(seed_1);

        let mut seed_2 = [0u8; 32];
        seed_2.iter_mut().for_each(|t| *t = rand.random_u8());
        let rand_2 = ChaCha8Rng::from_seed(seed_2);

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

        init::LateResources { usart: uarrr, opt_rng: Some((rand_1, rand_2)), led1, led2 }
    }

    #[idle(resources = [opt_rng, led1, led2])]
    fn idle(ctx: idle::Context) -> ! {
        rtic::pend(Interrupt::UARTE0_UART0);

        let (rand_1, rand_2) = ctx.resources.opt_rng.take().unwrap();

        let disco_socket = DISPATCH
            .register_port(DISCOVERY_PORT).unwrap();
        let token_socket = DISPATCH
            .register_port(TOKEN_PORT).unwrap();

        let mut sub_disco: Discovery<GlobalRollingTimer, _> =
            Discovery::new(rand_1, &DISPATCH, disco_socket, &BSLAB);
        let sub_disco_future = sub_disco.obtain_addr();
        pin_mut!(sub_disco_future);

        let mut sub_token: Token<GlobalRollingTimer, _> =
            Token::new(rand_2, &DISPATCH, token_socket, &BSLAB);
        let sub_token_future = sub_token.poll();
        pin_mut!(sub_token_future);

        let mut cas_sub_disco = Cassette::new(sub_disco_future);
        let mut cas_sub_token = Cassette::new(sub_token_future);

        let mut addr_oneshot = false;
        let timer = GlobalRollingTimer::default();
        let mut endshot = None;

        loop {
            if let Some(end) = endshot {
                cas_sub_token.poll_on();
                DISPATCH.process_messages();
                continue;
            }

            // Process messages
            DISPATCH.process_messages();

            // Check the actual tasks
            if let Some(msg) = cas_sub_disco.poll_on() {
                if let Ok(addr) = msg {
                    defmt::info!("got address! {=u8}", addr);
                } else {
                    defmt::error!("WAT?!?");
                }

                let now = timer.get_ticks();
                endshot = Some(now);
            }

            cas_sub_token.poll_on();

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
