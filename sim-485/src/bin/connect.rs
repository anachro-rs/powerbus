use std::num::NonZeroU8;
use std::thread::{spawn, sleep};
use std::time::{Duration, Instant};
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
// use postcard

use cassette::{Cassette, pin_mut};
use rand::thread_rng;

fn main() {
    let mut arc_bus = Rs485Bus::new_arc();

    let mut dev_1 = Rs485Device::new(&arc_bus);
    let mut dev_2 = Rs485Device::new(&arc_bus);


    let dom = DummyDom { dev: dev_1 };
    let dom_mtx = AsyncDomMutex::new(dom);

    let sub = DummySub { dev: dev_2 };
    let sub_mtx = AsyncSubMutex::new(sub);

    let mut dom_disco: DomDiscovery<GlobalRollingTimer, DummyDom, _> = DomDiscovery::new(dom_mtx, thread_rng());
    let dom_disco_future = dom_disco.poll();
    pin_mut!(dom_disco_future);

    let mut sub_disco: SubDiscovery<GlobalRollingTimer, DummySub, _> = SubDiscovery::new(sub_mtx, thread_rng());
    let sub_disco_future = sub_disco.obtain_addr();
    pin_mut!(sub_disco_future);

    let mut cas_dom = Cassette::new(dom_disco_future);
    let mut cas_sub = Cassette::new(sub_disco_future);

    let mut start = Instant::now();

    loop {
        // Check the actual tasks
        cas_dom.poll_on();
        if let Some(Ok(addr)) = cas_sub.poll_on() {
            panic!("address! {}", addr);
        }

        // Rate limiting
        sleep(Duration::from_micros(500));
    }
}

#[derive(Debug)]
struct DummyDom {
    dev: Rs485Device,
}

#[derive(Debug)]
struct DummySub {
    dev: Rs485Device,
}

impl SubInterface for DummySub {
    fn send_blocking<'a>(&mut self, msg: anachro_485::icd::BusSubMessage<'a>) -> Result<(), anachro_485::icd::BusSubMessage<'a>> {
        println!("SUB: {:?}", msg);
        let ser_msg = postcard::to_stdvec_cobs(&msg).map_err(|_| msg)?;
        println!("SUB: {:?}", ser_msg);
        Ok(())
    }

    fn pop(&mut self) -> Option<anachro_485::icd::BusDomMessage<'static>> {
        todo!()
    }
}

impl DomInterface for DummyDom {
    fn send_blocking<'a>(&mut self, msg: anachro_485::icd::BusDomMessage<'a>) -> Result<(), anachro_485::icd::BusDomMessage<'a>> {
        println!("DOM: {:?}", msg);
        let ser_msg = postcard::to_stdvec_cobs(&msg).map_err(|_| msg)?;
        println!("DOM: {:?}", ser_msg);
        Ok(())
    }

    fn pop(&mut self) -> Option<anachro_485::icd::BusSubMessage<'static>> {
        println!("Fake pop!");
        None
    }
}
