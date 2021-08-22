use std::num::NonZeroU8;
use std::thread::{spawn, sleep};
use std::time::{Duration, Instant};
use sim_485::groundhog_sim::GlobalRollingTimer;
use anachro_485::dom::{
    discover::Discovery,
    DomInterface,
    AsyncDomMutex,
};

use cassette::{Cassette, pin_mut};
use rand::thread_rng;

fn main() {
    let x = DummyDom {};
    let mtx = AsyncDomMutex::new(x);


    let mut disco: Discovery<GlobalRollingTimer, DummyDom, _> = Discovery::new(mtx.clone(), thread_rng());
    let disco_future = disco.poll();
    pin_mut!(disco_future);

    let mut cas = Cassette::new(disco_future);

    let mut start = Instant::now();
    let mut mtxgrd = None;

    loop {
        // TODO: Temp test for mutex
        if start.elapsed() >= Duration::from_secs(3) {
            if let Some(_g) = mtxgrd.take() {
                println!("Releasing...");
            } else {
                println!("Locking...");
                let f = mtx.lock_bus();
                pin_mut!(f);
                mtxgrd = Some(Cassette::new(f).block_on());
            }
            start = Instant::now();
        }

        // Check the actual tasks
        cas.poll_on();

        // Rate limiting
        sleep(Duration::from_micros(500));
    }
}

#[derive(Debug, Clone)]
struct DummyDom {

}

impl DomInterface for DummyDom {
    fn send_blocking(&mut self, msg: anachro_485::icd::BusDomMessage) -> Result<(), anachro_485::icd::BusDomMessage> {
        println!("Fake push!");
        Ok(())
    }

    fn pop(&mut self) -> Option<anachro_485::icd::BusSubMessage> {
        println!("Fake pop!");
        None
    }
}
