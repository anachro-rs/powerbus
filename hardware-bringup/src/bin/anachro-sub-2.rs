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
use anachro_485::{dispatch::{IoQueue, Dispatch}, dom::MANAGEMENT_PORT};
use anachro_485::icd::{SLAB_SIZE, TOTAL_SLABS};
use byte_slab::BSlab;
use groundhog_nrf52::GlobalRollingTimer;
use uarte_485::{Pin485, Uarte485};

use anachro_485::sub::discover::Discovery;
use cassette::{pin_mut, Cassette};

static IOQ: IoQueue = IoQueue::new();
static BSLAB: BSlab<TOTAL_SLABS, SLAB_SIZE> = BSlab::new();
static DISPATCH: Dispatch<8> = Dispatch::new(&IOQ, &BSLAB);

#[rtic::app(device = nrf52840_hal::pac, peripherals = true, monotonic = groundhog_nrf52::GlobalRollingTimer)]
const APP: () = {
    struct Resources {
        usart: Uarte485<TIMER2, Ppi3, UARTE0>,
        opt_rng: Option<Rng>,
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
        let _ = pins.rs1_de.into_push_pull_output(Level::Low);      // Disabled
        let _ = pins.rs1_re_n.into_push_pull_output(Level::High);   // Disabled
        let ppi = PpiParts::new(board.PPI);

        let rand = Rng::new(board.RNG);

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
        );

        init::LateResources { usart: uarrr, opt_rng: Some(rand) }
    }

    #[idle(resources = [opt_rng])]
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
            cas_sub_disco.poll_on();

            if !addr_oneshot {
                if let Some(addr) = DISPATCH.get_addr() {
                    addr_oneshot = true;
                    defmt::info!("Got address: {=u8}", addr);
                }
            }
        }
    }

    #[task(binds = UARTE0_UART0, resources = [usart])]
    fn uarte(ctx: uarte::Context) {
        // defmt::warn!("INT: uarte");
        ctx.resources.usart.uarte_interrupt();
    }

    #[task(binds = TIMER2)]
    fn timer(_ctx: timer::Context) {
        // TODO: It looks like we might have a spurious timer interrupt?
        // defmt::warn!("INT: timer");
        rtic::pend(Interrupt::UARTE0_UART0);
    }
};
