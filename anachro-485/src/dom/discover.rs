use crate::{async_sleep_millis, dispatch::{DispatchSocket, LocalPacket}, icd::{AddrPort, BusDomPayload, BusSubPayload, SLAB_SIZE, TOTAL_SLABS, VecAddr}, receive_timeout_micros};

use core::{
    iter::FromIterator,
    marker::PhantomData,
};

use byte_slab::BSlab;
use groundhog::RollingTimer;
use heapless::{FnvIndexMap, FnvIndexSet, Vec};
use rand::Rng;

use crate::dom::AddrTable32;

use super::MANAGEMENT_PORT;

pub struct Discovery<R, A>
where
    R: RollingTimer<Tick = u32> + Default,
    A: Rng,
{
    _timer: PhantomData<R>,
    socket: DispatchSocket<'static>,
    table: AddrTable32,
    rand: A,
    boost_mode: bool,
    alloc: &'static BSlab<TOTAL_SLABS, SLAB_SIZE>,
}

impl<R, A> Discovery<R, A>
where
    R: RollingTimer<Tick = u32> + Default,
    A: Rng,
{
    pub fn new(socket: DispatchSocket<'static>, rand: A, alloc: &'static BSlab<TOTAL_SLABS, SLAB_SIZE>) -> Self {
        Self {
            _timer: PhantomData,
            socket,
            rand,
            table: AddrTable32::new(),
            boost_mode: true,
            alloc
        }
    }

    pub async fn poll(&mut self) -> ! {
        let timer = R::default();
        let start = timer.get_ticks();
        self.boost_mode = true;

        loop {
            if self.boost_mode && timer.millis_since(start) >= 5000 {
                self.boost_mode = false;
            }

            if !self.boost_mode {
                async_sleep_millis::<R>(timer.get_ticks(), 1000u32).await;
            }

            match self.poll_inner().await {
                Ok(0) => {
                    if !self.boost_mode {
                        async_sleep_millis::<R>(timer.get_ticks(), 2000u32).await;
                    }
                }
                Ok(_) => println!("Poll good!"),
                Err(_) => println!("Poll bad!"),
            }
        }
    }

    pub async fn poll_inner(&mut self) -> Result<usize, ()> {
        let avail_addrs = { self.table.reserve_all_addrs() };
        let timer = R::default();

        if avail_addrs.is_empty() {
            return Err(());
        }

        // Broadcast initial
        let readies = self.broadcast_initial(&avail_addrs).await?;
        if readies.is_empty() {
            return Ok(0);
        }
        println!("READIES: {:?}", readies);

        if !self.boost_mode {
            async_sleep_millis::<R>(timer.get_ticks(), 1000u32).await;
        }

        let steadies = self.ping_readies(&readies).await?;
        println!("STEADIES: {:?}", steadies);
        if steadies.is_empty() {
            return Ok(0);
        }

        if !self.boost_mode {
            async_sleep_millis::<R>(timer.get_ticks(), 1000u32).await;
        }

        let gos = self.ping_readies(&steadies).await?;
        println!("GOs: {:?}", gos);

        let table = &mut self.table;
        gos.iter()
            .try_for_each(|g| table.commit_reserved_addr(*g))?;

        Ok(gos.len())
    }

    pub async fn ping_readies(&mut self, readies: &[u8]) -> Result<Vec<u8, 32>, ()> {
        let dom_random = self.rand.gen();
        let timer = R::default();
        let mut results = Vec::new();

        'outer: for ready in readies {
            let mut got = false;
            let payload = BusDomPayload::PingReq { random: dom_random, min_wait_us: 1_000, max_wait_us: 10_000 };

            let msg = LocalPacket::from_parts_with_alloc(
                payload,
                AddrPort::from_parts(VecAddr::local_dom_addr(), MANAGEMENT_PORT),
                AddrPort::from_parts(VecAddr::from_local_addr(*ready), MANAGEMENT_PORT),
                self.alloc,
            ).ok_or(())?;

            self.socket.try_send(msg).map_err(drop)?;
            let start = timer.get_ticks();

            'inner: loop {
                let maybe_msg = receive_timeout_micros::<R, BusSubPayload>(&mut self.socket, start, 20_000u32).await;

                let msg = match maybe_msg {
                    Some(msg) => msg,
                    None => break 'inner,
                };

                if msg.body.validate_ping_ack(&msg.hdr, dom_random).is_ok() {
                    if got {
                        continue 'outer;
                    } else {
                        got = true;
                    }
                }
            }

            if got {
                println!("yey!!!: {}", ready);
                results.push(*ready).map_err(drop)?;
            }
        }

        Ok(results)
    }

    pub async fn broadcast_initial(&mut self, avail_addrs: &[u8]) -> Result<Vec<u8, 32>, ()> {
        let timer = R::default();

        let dom_random = self.rand.gen();

        let payload = BusDomPayload::DiscoverInitial {
            random: dom_random,
            min_wait_us: 10_000,
            max_wait_us: 100_000,
            offers: Vec::from_iter(avail_addrs.iter().cloned()),
        };

        let msg = LocalPacket::from_parts_with_alloc(
            payload,
            AddrPort::from_parts(VecAddr::local_dom_addr(), MANAGEMENT_PORT),
            AddrPort::from_parts(VecAddr::local_broadcast_addr(), MANAGEMENT_PORT),
            self.alloc,
        ).ok_or(())?;

        self.socket.try_send(msg).map_err(drop)?;

        // Start the receive
        let start = timer.get_ticks();
        let mut resps = Vec::<_, 32>::new();

        // Collect until timeout, or max messages received
        while !resps.is_full() {
            let maybe_msg = receive_timeout_micros::<R, BusSubPayload>(&mut self.socket, start, 200_000u32).await;

            if let Some(msg) = maybe_msg {
                resps.push(msg).map_err(drop)?;
            } else {
                break;
            }
        }

        // println!("DOM RESPS: {:?}", resps);

        let mut offered = FnvIndexSet::<u8, 32>::new();
        let mut seen = FnvIndexSet::<u8, 32>::new();
        let mut dupes = FnvIndexSet::<u8, 32>::new();

        avail_addrs
            .iter()
            .try_for_each::<_, Result<_, u8>>(|a| {
                offered.insert(*a)?;
                Ok(())
            })
            .map_err(drop)?;

        let mut response_pairs = FnvIndexMap::<_, _, 32>::from_iter(
            resps
                .iter()
                // Remove any items that don't check out
                // .inspect(|r| println!("START: {:?}", r))
                .filter_map(|resp| resp.body.validate_discover_ack_addr(&resp.hdr, dom_random).ok())
                .inspect(|r| println!("FM1: {:?}", r))
                // Remove any items that weren't offered
                .filter(|(resp_addr, _)| offered.contains(resp_addr))
                .map(|(addr, sub_random)| {
                    // If the set did not have this value present, true is returned.
                    // If the set did have this value present, false is returned.
                    let new_addr = seen.insert(addr)?;
                    if !new_addr {
                        let _ = dupes.insert(addr)?;
                    }
                    Ok((addr, sub_random))
                })
                .filter_map(Result::<_, u8>::ok),
        );

        println!("RPs: {:?}", response_pairs);
        println!("DUPES: {:?}", dupes);

        // Remove any duplicates that have been seen
        dupes.iter().for_each(|d| {
            let _ = response_pairs.remove(d);
        });

        println!("RPs: {:?}", response_pairs);

        let mut accepted = Vec::<u8, 32>::new();
        // ACK acceptable response pairs
        for (addr, sub_random) in response_pairs.iter() {
            println!("ACCEPTING: {:?}", addr);
            if let Ok(_) = accepted.push(*addr) {

                let msg = BusDomPayload::generate_discover_ack_ack(
                    *addr,
                    self.rand.gen(),
                    *sub_random,
                );

                let msg = LocalPacket::from_parts_with_alloc(
                    msg.body,
                    msg.hdr.src,
                    msg.hdr.dst,
                    self.alloc,
                ).ok_or(())?;

                self.socket.try_send(msg).map_err(drop)?;
            }
        }

        Ok(accepted)
    }
}
