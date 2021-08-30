use std::ops::{Deref, DerefMut};
use std::thread::{sleep, spawn};
use std::time::Duration;
use sim_485::{Rs485Bus, Rs485Device};
use sim_485::groundhog_sim::GlobalRollingTimer;
use anachro_485::{
    dom::{
        discover::Discovery as DomDiscovery,
        DomInterface,
        AsyncDomMutex,
    },
    sub::{
        discover::Discovery as SubDiscovery,
        SubInterface,
        AsyncSubMutex,
    },
};
use byte_slab::BSlab;
// use postcard

use cassette::{Cassette, pin_mut};
use rand::thread_rng;
use anachro_485::icd::{TOTAL_SLABS, SLAB_SIZE};

static SLAB: BSlab<TOTAL_SLABS, SLAB_SIZE> = BSlab::new();

fn main() {
    SLAB.init().unwrap();
    let arc_bus = Rs485Bus::new_arc();

    let dev_1 = Rs485Device::new(&arc_bus);

    let mut dev_2 = Rs485Device::new(&arc_bus);
    let mut dev_3 = Rs485Device::new(&arc_bus);
    let mut dev_4 = Rs485Device::new(&arc_bus);
    let mut dev_5 = Rs485Device::new(&arc_bus);

    dev_2.enable_listen();
    dev_3.enable_listen();
    dev_4.enable_listen();
    dev_5.enable_listen();


    let dom = DummyDom { dev: dev_1, carry: vec![] };
    let dom_mtx = AsyncDomMutex::new(dom);

    let sub_1 = DummySub { dev: dev_2, carry: vec![] };
    let sub_mtx_1 = AsyncSubMutex::new(sub_1);
    let sub_2 = DummySub { dev: dev_3, carry: vec![] };
    let sub_mtx_2 = AsyncSubMutex::new(sub_2);
    let sub_3 = DummySub { dev: dev_4, carry: vec![] };
    let sub_mtx_3 = AsyncSubMutex::new(sub_3);
    let sub_4 = DummySub { dev: dev_5, carry: vec![] };
    let sub_mtx_4 = AsyncSubMutex::new(sub_4);

    let mut dom_disco: DomDiscovery<GlobalRollingTimer, DummyDom, _> = DomDiscovery::new(dom_mtx, thread_rng());
    let dom_disco_future = dom_disco.poll();
    pin_mut!(dom_disco_future);

    let mut cas_dom = Cassette::new(dom_disco_future);

    let sub_1_hdl = spawn(move || {
        let mut sub_disco_1: SubDiscovery<GlobalRollingTimer, DummySub, _> = SubDiscovery::new(sub_mtx_1, thread_rng());
        let sub_disco_future_1 = sub_disco_1.obtain_addr();
        pin_mut!(sub_disco_future_1);
        let mut cas_sub_1 = Cassette::new(sub_disco_future_1);
        let mut cas_sub_1_done = false;

        loop {
            if !cas_sub_1_done {
                if let Some(x) = cas_sub_1.poll_on() {
                    match x {
                        Ok(y) => {
                            cas_sub_1_done = true;
                            println!("cas_sub_1 addr: {:?}", y);
                        }
                        Err(e) => panic!("err! {:?}", e),
                    }
                }
            }

            // Rate limiting
            sleep(Duration::from_micros(500));
        }
    });

    let sub_2_hdl = spawn(move || {
        let mut sub_disco_2: SubDiscovery<GlobalRollingTimer, DummySub, _> = SubDiscovery::new(sub_mtx_2, thread_rng());
        let sub_disco_future_2 = sub_disco_2.obtain_addr();
        pin_mut!(sub_disco_future_2);

        let mut cas_sub_2 = Cassette::new(sub_disco_future_2);

        let mut cas_sub_2_done = false;

        loop {
            if !cas_sub_2_done {
                if let Some(x) = cas_sub_2.poll_on() {
                    match x {
                        Ok(y) => {
                            cas_sub_2_done = true;
                            println!("cas_sub_2 addr: {:?}", y);
                        }
                        Err(e) => panic!("err! {:?}", e),
                    }
                }
            }

            // Rate limiting
            sleep(Duration::from_micros(500));
        }
    });

    let sub_3_hdl = spawn(move || {
        let mut sub_disco_3: SubDiscovery<GlobalRollingTimer, DummySub, _> = SubDiscovery::new(sub_mtx_3, thread_rng());
        let sub_disco_future_3 = sub_disco_3.obtain_addr();
        pin_mut!(sub_disco_future_3);
        let mut cas_sub_3 = Cassette::new(sub_disco_future_3);
        let mut cas_sub_3_done = false;

        loop {
            if !cas_sub_3_done {
                if let Some(x) = cas_sub_3.poll_on() {
                    match x {
                        Ok(y) => {
                            cas_sub_3_done = true;
                            println!("cas_sub_3 addr: {:?}", y);
                        }
                        Err(e) => panic!("err! {:?}", e),
                    }
                }
            }

            // Rate limiting
            sleep(Duration::from_micros(500));
        }
    });

    let sub_4_hdl = spawn(move || {
        let mut sub_disco_4: SubDiscovery<GlobalRollingTimer, DummySub, _> = SubDiscovery::new(sub_mtx_4, thread_rng());
        let sub_disco_future_4 = sub_disco_4.obtain_addr();
        pin_mut!(sub_disco_future_4);
        let mut cas_sub_4 = Cassette::new(sub_disco_future_4);
        let mut cas_sub_4_done = false;

        loop {
            if !cas_sub_4_done {
                if let Some(x) = cas_sub_4.poll_on() {
                    match x {
                        Ok(y) => {
                            cas_sub_4_done = true;
                            println!("cas_sub_4 addr: {:?}", y);
                        }
                        Err(e) => panic!("err! {:?}", e),
                    }
                }
            }

            // Rate limiting
            sleep(Duration::from_micros(500));
        }
    });

    loop {
        // Check the actual tasks
        cas_dom.poll_on();

        // Rate limiting
        sleep(Duration::from_micros(500));
    }
}

struct DummyDom {
    dev: Rs485Device,
    carry: Vec<u8>,
}

struct DummySub {
    dev: Rs485Device,
    carry: Vec<u8>,
}

impl SubInterface for DummySub {
    fn send_blocking<'a>(&mut self, msg: anachro_485::icd::BusSubMessage<'a>) -> Result<(), anachro_485::icd::BusSubMessage<'a>> {
        println!("SUB: {:?}", msg);
        let ser_msg = postcard::to_stdvec_cobs(&msg).map_err(|_| msg)?;
        self.dev.disable_listen();
        self.dev.enable_transmit();
        self.dev.send(&ser_msg);
        self.dev.disable_transmit();
        self.dev.enable_listen();
        Ok(())
    }

    fn pop(&mut self) -> Option<anachro_485::icd::BusDomMessage<'static>> {
        self.carry.extend_from_slice(&self.dev.receive());

        let pos = self.carry.iter().position(|b| *b == 0)? + 1;
        let mut remain = self.carry.split_off(pos);


        core::mem::swap(&mut self.carry, &mut remain);
        let current = remain;

        if pos >= SLAB_SIZE {
            println!("TOO BIG");
            None
        } else {
            let mut sbox = SLAB.alloc_box()?;
            sbox[..pos].copy_from_slice(&current);

            use cobs::decode_in_place;
            let len = decode_in_place(&mut sbox[..pos]).ok()?;
            let sarc = sbox.into_arc();
            let msg: anachro_485::icd::BusDomMessage = postcard::from_bytes(&sarc[..len]).ok()?;

            msg.reroot(&sarc)
        }
    }
}

impl DomInterface for DummyDom {
    fn send_blocking<'a>(&mut self, msg: anachro_485::icd::BusDomMessage<'a>) -> Result<(), anachro_485::icd::BusDomMessage<'a>> {
        println!("DOM: {:?}", msg);
        let ser_msg = postcard::to_stdvec_cobs(&msg).map_err(|_| msg)?;
        self.dev.disable_listen();
        self.dev.enable_transmit();
        self.dev.send(&ser_msg);
        self.dev.disable_transmit();
        self.dev.enable_listen();
        Ok(())
    }

    fn pop(&mut self) -> Option<anachro_485::icd::BusSubMessage<'static>> {
        self.carry.extend_from_slice(&self.dev.receive());

        let pos = self.carry.iter().position(|b| *b == 0)? + 1;
        let mut remain = self.carry.split_off(pos);
        core::mem::swap(&mut self.carry, &mut remain);
        let current = remain;

        if pos >= SLAB_SIZE {
            println!("TOO BIG");
            None
        } else {
            let mut sbox = SLAB.alloc_box()?;
            sbox[..pos].copy_from_slice(&current);

            use cobs::decode_in_place;
            let len = decode_in_place(&mut sbox[..pos]).ok()?;
            let sarc = sbox.into_arc();

            let msg: anachro_485::icd::BusSubMessage = postcard::from_bytes(&sarc[..len]).ok()?;
            msg.reroot(&sarc)
        }
    }
}
