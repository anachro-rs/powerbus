use std::{iter::FromIterator, sync::Arc, thread::{JoinHandle, sleep, spawn}, time::Duration};

use sim_485::{Rs485Bus, Rs485Device, groundhog_sim::GlobalRollingTimer};
use anachro_485::{
    icd::{TOTAL_SLABS, SLAB_SIZE},
    dom::{
        discover::Discovery as DomDiscovery,
        DomInterface,
        AsyncDomMutex,
        ping::Ping as DomPing,
    },
    sub::{
        discover::Discovery as SubDiscovery,
        SubInterface,
        AsyncSubMutex,
    },
};

use byte_slab::BSlab;
use cassette::{Cassette, pin_mut};
use rand::thread_rng;

static SLAB: BSlab<TOTAL_SLABS, SLAB_SIZE> = BSlab::new();

fn main() {
    SLAB.init().unwrap();
    let arc_bus = Rs485Bus::new_arc();

    let mut network = Vec::from_iter([
        make_me_a_dom(&arc_bus),

        make_me_a_sub(&arc_bus),
        make_me_a_sub(&arc_bus),
        make_me_a_sub(&arc_bus),
        make_me_a_sub(&arc_bus),

        make_me_a_sub(&arc_bus),
        make_me_a_sub(&arc_bus),
        make_me_a_sub(&arc_bus),
        make_me_a_sub(&arc_bus),

        make_me_a_sub(&arc_bus),
        make_me_a_sub(&arc_bus),
        make_me_a_sub(&arc_bus),
        make_me_a_sub(&arc_bus),

        make_me_a_sub(&arc_bus),
        make_me_a_sub(&arc_bus),
        make_me_a_sub(&arc_bus),
        make_me_a_sub(&arc_bus),

        make_me_a_sub(&arc_bus),
        make_me_a_sub(&arc_bus),
        make_me_a_sub(&arc_bus),
        make_me_a_sub(&arc_bus),

        make_me_a_sub(&arc_bus),
        make_me_a_sub(&arc_bus),
        make_me_a_sub(&arc_bus),
        make_me_a_sub(&arc_bus),

        make_me_a_sub(&arc_bus),
        make_me_a_sub(&arc_bus),
        make_me_a_sub(&arc_bus),
        make_me_a_sub(&arc_bus),

        make_me_a_sub(&arc_bus),
        make_me_a_sub(&arc_bus),
        make_me_a_sub(&arc_bus),
        make_me_a_sub(&arc_bus),
    ]);

    network.drain(..).for_each(|h| {
        let _ = h.join();
    });
}

fn make_me_a_dom(arc_bus: &Arc<Rs485Bus>) -> JoinHandle<()> {
    let dev_1 = Rs485Device::new(&arc_bus);

    spawn(move || {
        let dom = DummyDom { dev: dev_1, carry: vec![] };
        let dom_mtx = AsyncDomMutex::new(dom);
        let dom_mtx_2 = dom_mtx.clone();


        let mut dom_disco: DomDiscovery<GlobalRollingTimer, DummyDom, _> = DomDiscovery::new(dom_mtx, thread_rng());
        let dom_disco_future = dom_disco.poll();
        pin_mut!(dom_disco_future);

        let mut dom_ping: DomPing<GlobalRollingTimer, DummyDom, _> = DomPing::new(dom_mtx_2, thread_rng());
        let dom_ping_future = dom_ping.poll();
        pin_mut!(dom_ping_future);

        let mut cas_dom = Cassette::new(dom_disco_future);
        let mut cas_dom_2 = Cassette::new(dom_ping_future);

        loop {
            // Check the actual tasks
            cas_dom.poll_on();
            cas_dom_2.poll_on();

            // Rate limiting
            sleep(Duration::from_micros(500));
        }
    })
}

fn make_me_a_sub(arc_bus: &Arc<Rs485Bus>) -> JoinHandle<()> {
    let mut dev_2 = Rs485Device::new(arc_bus);
    dev_2.enable_listen();

    spawn(move || {
        let sub_1 = DummySub { dev: dev_2, carry: vec![] };
        let sub_mtx_1 = AsyncSubMutex::new(sub_1);

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
    })
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
