use std::num::NonZeroU8;
use std::thread::{spawn, sleep};
use std::time::{Duration, Instant};
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

use cassette::{Cassette, pin_mut};
use rand::thread_rng;

fn main() {
    let dom = DummyDom {};
    let dom_mtx = AsyncDomMutex::new(dom);

    let sub = DummySub {};
    let sub_mtx = AsyncSubMutex::new(sub);

    let mut dom_disco: DomDiscovery<GlobalRollingTimer, DummyDom, _> = DomDiscovery::new(dom_mtx.clone(), thread_rng());
    let dom_disco_future = dom_disco.poll();
    pin_mut!(dom_disco_future);

    let mut sub_disco: SubDiscovery<GlobalRollingTimer, DummySub, _> = SubDiscovery::new(sub_mtx.clone(), thread_rng());
    let sub_disco_future = sub_disco.obtain_addr();
    pin_mut!(sub_disco_future);

    let mut cas_dom = Cassette::new(dom_disco_future);
    let mut cas_sub = Cassette::new(sub_disco_future);

    let mut start = Instant::now();
    let mut mtxgrd = None;

    loop {
        // TODO: Temp test for mutex
        if start.elapsed() >= Duration::from_secs(3) {
            if let Some(_g) = mtxgrd.take() {
                println!("Releasing...");
            } else {
                println!("Locking...");
                let f = dom_mtx.lock_bus();
                pin_mut!(f);
                mtxgrd = Some(Cassette::new(f).block_on());
            }
            start = Instant::now();
        }

        // Check the actual tasks
        cas_dom.poll_on();
        if let Some(Ok(addr)) = cas_sub.poll_on() {
            panic!("address! {}", addr);
        }

        // Rate limiting
        sleep(Duration::from_micros(500));
    }
}

#[derive(Debug, Clone)]
struct DummyDom {

}

#[derive(Debug, Clone)]
struct DummySub {

}

impl SubInterface for DummySub {
    fn send_blocking(&mut self, msg: anachro_485::icd::BusSubMessage) -> Result<(), anachro_485::icd::BusDomMessage> {
        todo!()
    }

    fn pop(&mut self) -> Option<anachro_485::icd::BusDomMessage<'static>> {
        todo!()
    }
}

impl DomInterface for DummyDom {
    fn send_blocking(&mut self, msg: anachro_485::icd::BusDomMessage) -> Result<(), anachro_485::icd::BusDomMessage> {
        println!("Fake push!");
        println!("{:?}", msg);
        Ok(())
    }

    fn pop(&mut self) -> Option<anachro_485::icd::BusSubMessage<'static>> {
        println!("Fake pop!");
        None
    }
}
