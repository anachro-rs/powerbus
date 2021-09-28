use crate::{
    async_sleep_millis,
    dispatch::{DispatchSocket, LocalPacket},
    icd::{AddrPort, DomDiscoveryPayload, SubDiscoveryPayload, VecAddr, SLAB_SIZE, TOTAL_SLABS},
    receive_timeout_micros,
    timing::{
        DOM_BROADCAST_MAX_WAIT_US, DOM_BROADCAST_MIN_WAIT_US, DOM_PING_MAX_WAIT_US,
        DOM_PING_MIN_WAIT_US,
    },
};

use core::{iter::FromIterator, marker::PhantomData, ops::Deref};

use byte_slab::BSlab;
use groundhog::RollingTimer;
use heapless::{FnvIndexMap, FnvIndexSet, Vec};
use rand::Rng;

use crate::dom::AddrTable32;

use super::DISCOVERY_PORT;

pub struct Discovery<R, A>
where
    R: RollingTimer<Tick = u32> + Default,
    A: Rng,
{
    _timer: PhantomData<R>,
    socket: DispatchSocket<'static>,
    table: &'static AddrTable32,
    rand: A,
    boost_mode: bool,
    alloc: &'static BSlab<TOTAL_SLABS, SLAB_SIZE>,
    last_disc: Option<u32>,
}

impl<R, A> Discovery<R, A>
where
    R: RollingTimer<Tick = u32> + Default,
    A: Rng,
{
    pub fn new(
        socket: DispatchSocket<'static>,
        rand: A,
        alloc: &'static BSlab<TOTAL_SLABS, SLAB_SIZE>,
        table: &'static AddrTable32,
    ) -> Self {
        Self {
            _timer: PhantomData,
            socket,
            rand,
            table,
            boost_mode: true,
            alloc,
            last_disc: None,
        }
    }

    pub async fn poll(&mut self) -> ! {
        let timer = R::default();
        self.boost_mode = false;

        loop {
            // Boost until we haven't heard from a new device in the
            // last three seconds (once after boot)
            if let Some(ld) = self.last_disc {
                if self.boost_mode && timer.millis_since(ld) >= 3_000 {
                    self.boost_mode = false;
                    self.last_disc = None;
                }
            }

            if !self.boost_mode {
                async_sleep_millis::<R>(timer.get_ticks(), 1000u32).await;
            } else {
                async_sleep_millis::<R>(timer.get_ticks(), 100u32).await;
            }

            let ret = self.poll_inner().await;

            match ret {
                Ok(0) => {
                    if !self.boost_mode {
                        async_sleep_millis::<R>(timer.get_ticks(), 1000u32).await;
                    }
                }
                Ok(_) => {
                    defmt::info!("Poll good!")
                }
                Err(_) => {
                    defmt::info!("Poll bad!")
                }
            }
        }
    }

    pub async fn poll_inner(&mut self) -> Result<usize, ()> {
        let avail_addrs = self.table.get_available_addrs();
        let timer = R::default();

        if avail_addrs.is_empty() {
            return Err(());
        } else {
            defmt::info!("avail addrs: {}", avail_addrs.len());
        }

        // Broadcast initial
        let readies = self.broadcast_initial(&avail_addrs).await?;
        if readies.is_empty() {
            return Ok(0);
        }
        self.last_disc = Some(timer.get_ticks());
        defmt::info!("READIES: {:?}", readies.deref());

        if !self.boost_mode {
            async_sleep_millis::<R>(timer.get_ticks(), 1000u32).await;
        } else {
            async_sleep_millis::<R>(timer.get_ticks(), 100u32).await;
        }

        let steadies = self.ping_readies(&readies).await?;
        defmt::info!("STEADIES: {:?}", steadies.deref());
        if steadies.is_empty() {
            return Ok(0);
        }

        if !self.boost_mode {
            async_sleep_millis::<R>(timer.get_ticks(), 1000u32).await;
        } else {
            async_sleep_millis::<R>(timer.get_ticks(), 100u32).await;
        }

        let gos = self.ping_readies(&steadies).await?;
        defmt::info!("GOs: {:?}", gos.deref());

        let table = &mut self.table;
        gos.iter()
            .try_for_each(|g| table.commit_reserved_addr(*g))
            .unwrap();

        Ok(gos.len())
    }

    pub async fn ping_readies(&mut self, readies: &[u8]) -> Result<Vec<u8, 32>, ()> {
        let dom_random = self.rand.gen();
        let timer = R::default();
        let mut results = Vec::new();

        'outer: for ready in readies {
            let mut got = false;
            let payload = DomDiscoveryPayload::PingReq {
                random: dom_random,
                min_wait_us: DOM_PING_MIN_WAIT_US,
                max_wait_us: DOM_PING_MAX_WAIT_US,
            };

            let msg = LocalPacket::from_parts_with_alloc(
                payload,
                AddrPort::from_parts(VecAddr::local_dom_addr(), DISCOVERY_PORT),
                AddrPort::from_parts(VecAddr::from_local_addr(*ready), DISCOVERY_PORT),
                Some(DOM_PING_MAX_WAIT_US),
                self.alloc,
            )
            .ok_or(())?;

            self.socket.try_send_authd(msg).map_err(drop)?;
            let start = timer.get_ticks();

            'inner: loop {
                let maybe_msg = receive_timeout_micros::<R, SubDiscoveryPayload>(
                    &mut self.socket,
                    start,
                    DOM_PING_MAX_WAIT_US,
                )
                .await;

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
                defmt::info!("yey!!!: {}", ready);
                results.push(*ready).map_err(drop)?;
            }
        }

        // defmt::info!("grepme: {:?}", results);

        Ok(results)
    }

    pub async fn broadcast_initial(&mut self, avail_addrs: &[u8]) -> Result<Vec<u8, 32>, ()> {
        let timer = R::default();

        let dom_random = self.rand.gen();

        let payload = DomDiscoveryPayload::DiscoverInitial {
            random: dom_random,
            min_wait_us: DOM_BROADCAST_MIN_WAIT_US,
            max_wait_us: DOM_BROADCAST_MAX_WAIT_US,
            offers: Vec::from_iter(avail_addrs.iter().cloned()),
        };

        let msg = LocalPacket::from_parts_with_alloc(
            payload,
            AddrPort::from_parts(VecAddr::local_dom_addr(), DISCOVERY_PORT),
            AddrPort::from_parts(VecAddr::local_broadcast_addr(), DISCOVERY_PORT),
            Some(DOM_BROADCAST_MAX_WAIT_US),
            self.alloc,
        )
        .ok_or(())?;
        defmt::info!("BROADCAST!");
        self.socket.try_send_authd(msg).map_err(drop)?;

        // Start the receive
        let start = timer.get_ticks();
        let mut resps = Vec::<_, 32>::new();

        // Collect until timeout, or max messages received
        while !resps.is_full() {
            let maybe_msg = receive_timeout_micros::<R, SubDiscoveryPayload>(
                &mut self.socket,
                start,
                DOM_BROADCAST_MAX_WAIT_US,
            )
            .await;

            if let Some(msg) = maybe_msg {
                resps.push(msg).map_err(drop)?;
            } else {
                break;
            }
        }

        defmt::info!("DOM RESPS: {:?}", resps.deref().len());

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
                .filter_map(|resp| {
                    resp.body
                        .validate_discover_ack_addr(&resp.hdr, dom_random)
                        .ok()
                })
                // .inspect(|r| println!("FM1: {:?}", r))
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

        defmt::info!("RPs: {:?}", response_pairs.len());
        defmt::info!("DUPES: {:?}", dupes.len());

        // return Err(());

        // Remove any duplicates that have been seen
        dupes.iter().for_each(|d| {
            let _ = response_pairs.remove(d);
        });

        // defmt::info!("RPs: {:?}", response_pairs);

        let mut accepted = Vec::<u8, 32>::new();
        // ACK acceptable response pairs
        for (addr, sub_random) in response_pairs.iter() {
            defmt::info!("ACCEPTING: {:?}", addr);
            if let Ok(_) = accepted.push(*addr) {
                let msg = DomDiscoveryPayload::generate_discover_ack_ack(
                    *addr,
                    self.rand.gen(),
                    *sub_random,
                );

                let msg = LocalPacket::from_parts_with_alloc(
                    msg.body,
                    msg.hdr.src,
                    msg.hdr.dst,
                    None,
                    self.alloc,
                )
                .ok_or(())?;

                self.socket.try_send_authd(msg).map_err(drop)?;
            }
        }

        Ok(accepted)
    }
}
