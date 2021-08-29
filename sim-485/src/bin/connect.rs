use std::ops::{Deref, DerefMut};
use std::thread::sleep;
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

    dev_2.enable_listen();


    let dom = DummyDom { dev: dev_1, carry: vec![] };
    let dom_mtx = AsyncDomMutex::new(dom);

    let sub = DummySub { dev: dev_2, carry: vec![] };
    let sub_mtx = AsyncSubMutex::new(sub);

    let mut dom_disco: DomDiscovery<GlobalRollingTimer, DummyDom, _> = DomDiscovery::new(dom_mtx, thread_rng());
    let dom_disco_future = dom_disco.poll();
    pin_mut!(dom_disco_future);

    let mut sub_disco: SubDiscovery<GlobalRollingTimer, DummySub, _> = SubDiscovery::new(sub_mtx, thread_rng());
    let sub_disco_future = sub_disco.obtain_addr();
    pin_mut!(sub_disco_future);

    let mut cas_dom = Cassette::new(dom_disco_future);
    let mut cas_sub = Cassette::new(sub_disco_future);

    loop {
        // Check the actual tasks
        cas_dom.poll_on();

        if let Some(x) = cas_sub.poll_on() {
            match x {
                Ok(y) => panic!("address! {:?}", y),
                Err(e) => panic!("err! {:?}", e),
            }
        }

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
