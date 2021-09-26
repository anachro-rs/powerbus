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

use anachro_485::dom::discover::Discovery;
use cassette::{pin_mut, Cassette};

static IOQ: IoQueue = IoQueue::new();
static BSLAB: BSlab<TOTAL_SLABS, SLAB_SIZE> = BSlab::new();

#[rtic::app(device = nrf52840_hal::pac, peripherals = true, monotonic = groundhog_nrf52::GlobalRollingTimer)]
const APP: () = {
    struct Resources {
        usart: Uarte485<TIMER2, Ppi3, UARTE0>,
        dispatch: Dispatch<8>,
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

        let dispatch: Dispatch<8> = Dispatch::new(&IOQ, &BSLAB);
        dispatch.set_addr(0);


        init::LateResources { usart: uarrr, dispatch, opt_rng: Some(rand) }
    }

    #[idle(resources = [dispatch, opt_rng])]
    fn idle(ctx: idle::Context) -> ! {
        rtic::pend(Interrupt::UARTE0_UART0);

        let rand = ctx.resources.opt_rng.take().unwrap();

        let mgmt_socket = ctx
            .resources
            .dispatch
            .register_port(MANAGEMENT_PORT).unwrap();

        let mut dom_disco: Discovery<GlobalRollingTimer, _> =
            Discovery::new(mgmt_socket, rand, &BSLAB);
        let dom_disco_future = dom_disco.poll();
        pin_mut!(dom_disco_future);

        let mut cas_dom_disco = Cassette::new(dom_disco_future);

        loop {
            // Process messages
            ctx.resources.dispatch.process_messages();

            // Check the actual tasks
            cas_dom_disco.poll_on();

            // if let Some(msg) = IOQ.to_dispatch.dequeue() {
            //     let mut reply = false;
            //     match core::str::from_utf8(&msg.packet[..msg.len]) {
            //         Ok(strng) => {
            //             defmt::info!("Got: {:?}", strng);
            //             reply = true;
            //         }
            //         Err(_) => {
            //             defmt::warn!("Bad decode: {=usize} => {:?}", msg.len, &msg.packet[..msg.len]);
            //         }
            //     }

            //     if reply {
            //         match BSLAB.alloc_box() {
            //             Some(mut sbox) => {
            //                 const REPLY: &[u8] = b"->Pow!";
            //                 sbox[..REPLY.len()].copy_from_slice(REPLY);

            //                 let arc = sbox.into_arc();
            //                 let ssa = arc.sub_slice_arc(0, REPLY.len()).unwrap();
            //                 let mas = ManagedArcSlab::Owned(ssa);

            //                 IOQ.to_io.enqueue(mas).unwrap();
            //                 IOQ.io_send_auth.store(true, SeqCst);
            //             }
            //             None => {
            //                 defmt::warn!("Tried to reply, no alloc :(");
            //             }
            //         }
            //     }
            // }
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
