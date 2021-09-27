#![no_main]
#![no_std]

use hardware_bringup::{self as _, PowerBusPins};
use nrf52840_hal::{
    gpio::Level,
    pac::{Interrupt, TIMER2, UARTE0},
    ppi::{Parts as PpiParts, Ppi3},
    rng::Rng,
    Timer,
};
// use groundhog::RollingTimer;
use anachro_485::{dispatch::{IoQueue, Dispatch}, dom::{AddrTable32, DISCOVERY_PORT, TOKEN_PORT, token::Token}};
use anachro_485::icd::{SLAB_SIZE, TOTAL_SLABS};
use byte_slab::BSlab;
use groundhog_nrf52::GlobalRollingTimer;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use uarte_485::{Pin485, Uarte485, DefaultTo};

use anachro_485::dom::discover::Discovery;
use cassette::{pin_mut, Cassette};

static IOQ: IoQueue = IoQueue::new();
static BSLAB: BSlab<TOTAL_SLABS, SLAB_SIZE> = BSlab::new();
static ADDR_TABLE: AddrTable32 = AddrTable32::new();

#[rtic::app(device = nrf52840_hal::pac, peripherals = true, monotonic = groundhog_nrf52::GlobalRollingTimer)]
const APP: () = {
    struct Resources {
        usart: Uarte485<TIMER2, Ppi3, UARTE0, GlobalRollingTimer>,
        dispatch: Dispatch<8>,
        opt_rng: Option<(ChaCha8Rng, ChaCha8Rng)>,
    }

    #[init]
    fn init(cx: init::Context) -> init::LateResources {
        defmt::info!("Hello, world!");
        defmt::info!("Dom on Port 2 (Cap)");
        BSLAB.init().unwrap();

        let board = cx.device;

        GlobalRollingTimer::init(board.TIMER0);
        let _timer = GlobalRollingTimer::default();
        let _timer_2 = Timer::new(board.TIMER1);

        let pins = PowerBusPins::from_ports(board.P0, board.P1);

        let _led1 = pins.led_1.into_push_pull_output(Level::High);
        let _led2 = pins.led_2.into_push_pull_output(Level::High);
        let _ = pins.rs1_de.into_push_pull_output(Level::Low);      // Disabled
        let _ = pins.rs1_re_n.into_push_pull_output(Level::High);   // Disabled
        let ppi = PpiParts::new(board.PPI);

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
                rs_di: pins.rs2_di.degrade(),
                rs_ro: pins.rs2_ro.degrade(),
                rs_de: pins.rs2_de.degrade(),
                rs_re_n: pins.rs2_re_n.degrade(),
            },
            IOQ.take_io_handle().unwrap(),
            DefaultTo::Sending,
        );

        let dispatch: Dispatch<8> = Dispatch::new(&IOQ, &BSLAB);
        dispatch.set_addr(0);


        init::LateResources { usart: uarrr, dispatch, opt_rng: Some((rand_1, rand_2)) }
    }

    #[idle(resources = [dispatch, opt_rng])]
    fn idle(ctx: idle::Context) -> ! {
        rtic::pend(Interrupt::UARTE0_UART0);

        let (rand_1, rand_2) = ctx.resources.opt_rng.take().unwrap();

        // DISCO
        let disco_socket = ctx
            .resources
            .dispatch
            .register_port(DISCOVERY_PORT).unwrap();

        let mut dom_disco: Discovery<GlobalRollingTimer, _> =
            Discovery::new(disco_socket, rand_1, &BSLAB, &ADDR_TABLE);
        let dom_disco_future = dom_disco.poll();
        pin_mut!(dom_disco_future);

        // GRANT
        let grant_socket = ctx
            .resources
            .dispatch
            .register_port(TOKEN_PORT).unwrap();

        let mut dom_token: Token<GlobalRollingTimer, _> =
            Token::new(grant_socket, rand_2, &BSLAB, &ADDR_TABLE);
        let dom_token_future = dom_token.poll();
        pin_mut!(dom_token_future);

        let mut cas_dom_disco = Cassette::new(dom_disco_future);
        let mut cas_dom_token = Cassette::new(dom_token_future);

        loop {
            // Check the actual tasks
            cas_dom_disco.poll_on();
            cas_dom_token.poll_on();

            // Process messages
            ctx.resources.dispatch.process_messages();
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
