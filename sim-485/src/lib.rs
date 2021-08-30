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
// const BUS_FREQUENCY_HZ: u64 = BITS_PER_DATA_BYTE * 1_000_000;
const BUS_FREQUENCY_HZ: u64 = 115200;

// Assuming 8N1 UART mode
const BITS_PER_DATA_BYTE: u64 = 9;
const NANOS_PER_BYTE: u64 = (1_000_000_000 * BITS_PER_DATA_BYTE) / BUS_FREQUENCY_HZ;

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
    senders: AtomicU32,
    sim_bus_ident: u32,
}

impl Rs485Bus {
    pub fn new_arc() -> Arc<Self> {
        let shared = Mutex::new(Rs485BusShared::default());
        let senders = AtomicU32::new(0);

        Arc::new(Self {
            shared,
            senders,
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
        let mut lock = self
            .shared
            .lock()
            .expect("Failed to lock mutex on data send");
        for byte in data {
            // ha ha! rate limiting!
            let senders_before_good = self.senders.load(SeqCst) == 1;
            sleep(Duration::from_nanos(NANOS_PER_BYTE));
            let senders_after_good = self.senders.load(SeqCst) == 1;

            for dev in lock.funnels.iter_mut() {
                if dev.listening.load(SeqCst) {
                    if senders_before_good && senders_after_good {
                        dev.sender.send(*byte).unwrap();
                    } else {
                        println!("Corrupted byte!");
                        dev.sender.send(0xAF).unwrap();
                    }
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
        let old = self.bus.senders.fetch_add(1, SeqCst);
        println!("Senders: {}", old + 1);
        self.sending = true;
    }

    pub fn disable_transmit(&mut self) {
        self.bus.senders.fetch_sub(1, SeqCst);
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
