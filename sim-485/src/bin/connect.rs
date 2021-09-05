use std::{iter::FromIterator, sync::Arc, thread::{JoinHandle, sleep, spawn}, time::Duration};

use sim_485::{Rs485Bus, Rs485Device, groundhog_sim::GlobalRollingTimer};
use anachro_485::{declare_dom, dispatch::{Dispatch, IoQueue}, dom::{DomHandle, MANAGEMENT_PORT, discover::Discovery as DomDiscovery}, icd::{TOTAL_SLABS, SLAB_SIZE}, sub::discover::Discovery as SubDiscovery};

use byte_slab::BSlab;
use cassette::{Cassette, pin_mut};
use rand::thread_rng;

fn main() {
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

    let dev_1 = Rs485Device::new(&arc_bus);

    // let mut dom_disco: DomDiscovery<GlobalRollingTimer, DummyDom, _> = DomDiscovery::new(dom_mtx, thread_rng());
    // let dom_disco_future = dom_disco.poll();
    // pin_mut!(dom_disco_future);

    // let mut hdl = DomHandle::new(&DISPATCH, dom_disco_future).unwrap();

    // declare_dom!({
    //     name: boop,
    //     dispatch: DISPATCH,
    // });

    // for _ in 0..10 {
    //     hdl.poll();
    //     boop.poll();
    // }

    network.drain(..).for_each(|h| {
        let _ = h.join();
    });
}

fn make_me_a_dom(arc_bus: &Arc<Rs485Bus>) -> JoinHandle<()> {
    let dev_1 = Rs485Device::new(&arc_bus);

    spawn(move || {

        let slab: &'static BSlab<TOTAL_SLABS, SLAB_SIZE> = Box::leak(Box::new(BSlab::new()));
        let ioq: &'static IoQueue = Box::leak(Box::new(IoQueue::new()));
        let dispatch: &'static Dispatch<8> = Box::leak(Box::new(Dispatch::new(ioq, slab)));
        slab.init().unwrap();

        let socket = dispatch.register_port(MANAGEMENT_PORT).unwrap();

        let mut dom_disco: DomDiscovery<GlobalRollingTimer, _> = DomDiscovery::new(socket, thread_rng(), slab);
        let dom_disco_future = dom_disco.poll();
        pin_mut!(dom_disco_future);

        let mut cas_dom = Cassette::new(dom_disco_future);

        loop {
            // Check the actual tasks
            cas_dom.poll_on();

            // Rate limiting
            sleep(Duration::from_micros(500));
        }
    })
}

fn make_me_a_sub(arc_bus: &Arc<Rs485Bus>) -> JoinHandle<()> {
    let mut dev_2 = Rs485Device::new(arc_bus);
    dev_2.enable_listen();

    spawn(move || {
        let slab: &'static BSlab<TOTAL_SLABS, SLAB_SIZE> = Box::leak(Box::new(BSlab::new()));
        let ioq: &'static IoQueue = Box::leak(Box::new(IoQueue::new()));
        let dispatch: &'static Dispatch<8> = Box::leak(Box::new(Dispatch::new(ioq, slab)));
        slab.init().unwrap();

        let socket = dispatch.register_port(MANAGEMENT_PORT).unwrap();

        let mut sub_disco_1: SubDiscovery<GlobalRollingTimer, _> = SubDiscovery::new(thread_rng(), socket, slab);
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

// impl SubInterface for DummySub {
//     fn send_blocking<'a>(&mut self, msg: anachro_485::icd::BusSubMessage<'a>) -> Result<(), anachro_485::icd::BusSubMessage<'a>> {
//         println!("SUB: {:?}", msg);
//         let ser_msg = postcard::to_stdvec_cobs(&msg).map_err(|_| msg)?;
//         self.dev.disable_listen();
//         self.dev.enable_transmit();
//         self.dev.send(&ser_msg);
//         self.dev.disable_transmit();
//         self.dev.enable_listen();
//         Ok(())
//     }

//     fn pop(&mut self) -> Option<anachro_485::icd::BusDomMessage<'static>> {
//         self.carry.extend_from_slice(&self.dev.receive());

//         let pos = self.carry.iter().position(|b| *b == 0)? + 1;
//         let mut remain = self.carry.split_off(pos);


//         core::mem::swap(&mut self.carry, &mut remain);
//         let current = remain;

//         if pos >= SLAB_SIZE {
//             println!("TOO BIG");
//             None
//         } else {
//             let mut sbox = SLAB.alloc_box()?;
//             sbox[..pos].copy_from_slice(&current);

//             use cobs::decode_in_place;
//             let len = decode_in_place(&mut sbox[..pos]).ok()?;
//             let sarc = sbox.into_arc();
//             let msg: anachro_485::icd::BusDomMessage = postcard::from_bytes(&sarc[..len]).ok()?;

//             msg.reroot(&sarc)
//         }
//     }
// }

// impl DomInterface for DummyDom {
//     fn send_blocking<'a>(&mut self, msg: anachro_485::icd::BusDomMessage<'a>) -> Result<(), anachro_485::icd::BusDomMessage<'a>> {
//         println!("DOM: {:?}", msg);
//         let ser_msg = postcard::to_stdvec_cobs(&msg).map_err(|_| msg)?;
//         self.dev.disable_listen();
//         self.dev.enable_transmit();
//         self.dev.send(&ser_msg);
//         self.dev.disable_transmit();
//         self.dev.enable_listen();
//         Ok(())
//     }

//     fn pop(&mut self) -> Option<anachro_485::icd::BusSubMessage<'static>> {
//         self.carry.extend_from_slice(&self.dev.receive());

//         let pos = self.carry.iter().position(|b| *b == 0)? + 1;
//         let mut remain = self.carry.split_off(pos);
//         core::mem::swap(&mut self.carry, &mut remain);
//         let current = remain;

//         if pos >= SLAB_SIZE {
//             println!("TOO BIG");
//             None
//         } else {
//             let mut sbox = SLAB.alloc_box()?;
//             sbox[..pos].copy_from_slice(&current);

//             use cobs::decode_in_place;
//             let len = decode_in_place(&mut sbox[..pos]).ok()?;
//             let sarc = sbox.into_arc();

//             let msg: anachro_485::icd::BusSubMessage = postcard::from_bytes(&sarc[..len]).ok()?;
//             msg.reroot(&sarc)
//         }
//     }
// }
