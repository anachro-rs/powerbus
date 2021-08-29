#![allow(unused_imports, dead_code)]

pub mod groundhog_sim;

use std::{
    sync::{
        atomic::{
            AtomicBool, AtomicU32, AtomicU8, AtomicUsize,
            Ordering::{self, SeqCst},
        },
        mpsc::{channel, Receiver, Sender},
        Arc, Mutex,
    },
    thread::sleep,
    time::Duration,
};

// TODO: NOT a constant frequency
const BUS_FREQUENCY_HZ: u64 = BITS_PER_DATA_BYTE * 1_000_000;

// Assuming 8N1 UART mode
const BITS_PER_DATA_BYTE: u64 = 9;
const NANOS_PER_BYTE: u64 = (1_000_000_000 * u8::BITS as u64) / BUS_FREQUENCY_HZ;

static BUS_CTR: AtomicU32 = AtomicU32::new(1);
static SIM_CTR: AtomicU32 = AtomicU32::new(1);

#[derive(Debug)]
pub struct Rs485Device {
    listening: Arc<AtomicBool>,
    receiver: Receiver<u8>,
    bus: Arc<Rs485Bus>,
    sim_dev_ident: u32,
    sending: bool,
}

#[derive(Debug)]
pub struct Rs485Bus {
    // TODO: Baud rate? Not a constant?
    shared: Mutex<Rs485BusShared>,
    sender: AtomicU32,
    sim_bus_ident: u32,
}

impl Rs485Bus {
    pub const INACTIVE_SENDER: u32 = 0;

    pub fn new_arc() -> Arc<Self> {
        let shared = Mutex::new(Rs485BusShared::default());
        let sender = AtomicU32::new(Self::INACTIVE_SENDER);

        Arc::new(Self {
            shared,
            sender,
            sim_bus_ident: BUS_CTR.fetch_add(1, Ordering::SeqCst),
        })
    }

    fn add_device(&self, funnel: DeviceFunnel) {
        let mut lock = self
            .shared
            .lock()
            .expect("Failed to lock mutex on device add");

        // Check we aren't adding a duplicate address
        let dupe = lock.funnels.iter().any(|f| f.sim_ident == funnel.sim_ident);
        assert!(!dupe, "DUPLICATE ADDR ADDED TO BUS");

        lock.funnels.push(funnel);
    }

    fn send_data(&self, data: &[u8]) {
        // The bus should be active at the time of sending
        assert_ne!(Self::INACTIVE_SENDER, self.sender.load(SeqCst));

        let mut lock = self
            .shared
            .lock()
            .expect("Failed to lock mutex on data send");
        for byte in data {
            // ha ha! rate limiting!
            sleep(Duration::from_nanos(NANOS_PER_BYTE));

            for dev in lock.funnels.iter_mut() {
                if dev.listening.load(SeqCst) {
                    dev.sender.send(*byte).unwrap();
                }
            }
        }
    }
}

#[derive(Default, Debug)]
struct Rs485BusShared {
    funnels: Vec<DeviceFunnel>,
}

#[derive(Debug)]
struct DeviceFunnel {
    sim_ident: u32,
    listening: Arc<AtomicBool>,
    sender: Sender<u8>,
}

impl Rs485Device {
    pub fn new(bus: &Arc<Rs485Bus>) -> Self {
        let listening = Arc::new(AtomicBool::new(false));
        let (prod, cons) = channel();
        let sim_ident = SIM_CTR.fetch_add(1, Ordering::SeqCst);
        let funnel = DeviceFunnel {
            sim_ident,
            listening: listening.clone(),
            sender: prod,
        };
        bus.add_device(funnel);

        Self {
            listening,
            receiver: cons,
            bus: bus.clone(),
            sim_dev_ident: sim_ident,
            sending: false,
        }
    }

    pub fn enable_listen(&mut self) {
        self.listening.store(true, SeqCst);
    }

    pub fn disable_listen(&mut self) {
        self.listening.store(false, SeqCst);
    }

    pub fn enable_transmit(&mut self) {
        let swappy = self.bus.sender.compare_exchange(
            Rs485Bus::INACTIVE_SENDER,
            self.sim_dev_ident,
            SeqCst,
            SeqCst,
        );
        assert!(swappy.is_ok(), "BUS FAULT - ACQUIRE");
        self.sending = true;
    }

    pub fn disable_transmit(&mut self) {
        let swappy = self.bus.sender.compare_exchange(
            self.sim_dev_ident,
            Rs485Bus::INACTIVE_SENDER,
            SeqCst,
            SeqCst,
        );
        assert!(swappy.is_ok(), "BUS FAULT - RELEASE");
        self.sending = false;
    }

    pub fn send(&mut self, data: &[u8]) {
        assert!(self.sending, "Sending without enabling transmit!");
        self.bus.send_data(data);
    }

    pub fn receive(&mut self) -> Vec<u8> {
        // TODO: Use a bounded channel instead to measure capacity?
        let mut payload = Vec::new();
        while let Ok(data) = self.receiver.try_recv() {
            payload.push(data);
        }
        payload
    }
}
