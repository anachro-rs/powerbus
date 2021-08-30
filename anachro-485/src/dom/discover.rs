use super::AsyncDomMutex;
use super::DomInterface;
use crate::async_sleep_millis;
use crate::dom;
use crate::icd::BusSubPayload;
use crate::icd::RefAddr;
use crate::icd::{BusDomMessage, BusDomPayload};
use byte_slab::ManagedArcSlab;
use core::iter::FromIterator;
use core::marker::PhantomData;
use core::ops::DerefMut;
use groundhog::RollingTimer;
use heapless::{FnvIndexMap, FnvIndexSet, Vec};
use rand::Rng;

pub struct Discovery<R, T, A>
where
    R: RollingTimer<Tick = u32> + Default,
    T: DomInterface,
    A: Rng,
{
    _timer: PhantomData<R>,
    mutex: AsyncDomMutex<T>,
    rand: A,
}

impl<R, T, A> Discovery<R, T, A>
where
    R: RollingTimer<Tick = u32> + Default,
    T: DomInterface,
    A: Rng,
{
    pub fn new(mutex: AsyncDomMutex<T>, rand: A) -> Self {
        Self {
            _timer: PhantomData,
            mutex,
            rand,
        }
    }

    pub async fn poll(&mut self) -> ! {
        let timer = R::default();
        loop {
            async_sleep_millis::<R>(timer.get_ticks(), 1000u32).await;

            match self.poll_inner().await {
                Ok(0) => {
                    async_sleep_millis::<R>(timer.get_ticks(), 2000u32).await;
                }
                Ok(_) => println!("Poll good!"),
                Err(_) => println!("Poll bad!"),
            }
        }
    }

    pub async fn poll_inner(&mut self) -> Result<usize, ()> {
        let avail_addrs = { self.mutex.lock_table().await.reserve_all_addrs() };
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

        async_sleep_millis::<R>(timer.get_ticks(), 1000u32).await;
        let steadies = self.ping_readies(&readies).await?;
        println!("STEADIES: {:?}", steadies);
        if steadies.is_empty() {
            return Ok(0);
        }

        async_sleep_millis::<R>(timer.get_ticks(), 1000u32).await;
        let gos = self.ping_readies(&steadies).await?;
        println!("GOs: {:?}", gos);

        let mut table = self.mutex.lock_table().await;
        gos.iter()
            .try_for_each(|g| table.commit_reserved_addr(*g))?;

        Ok(gos.len())
    }

    pub async fn ping_readies(&mut self, readies: &[u8]) -> Result<Vec<u8, 32>, ()> {
        let mut bus = self.mutex.lock_bus().await;
        let dom_random = self.rand.gen();
        let timer = R::default();
        let mut results = Vec::new();

        'outer: for ready in readies {
            let mut got = false;
            let payload = BusDomPayload::PingReq { random: dom_random, min_wait_us: 1_000, max_wait_us: 10_000 };
            let msg = BusDomMessage {
                src: RefAddr::local_dom_addr(),
                dst: RefAddr::from_local_addr(*ready),
                payload,
            };
            bus.send_blocking(msg).map_err(drop)?;
            let start = timer.get_ticks();

            'inner: loop {
                let maybe_msg =
                    super::receive_timeout_micros::<T, R>(bus.deref_mut(), start, 20_000u32).await;

                let msg = match maybe_msg {
                    Some(msg) => msg,
                    None => break 'inner,
                };

                if msg.validate_ping_ack(dom_random).is_ok() {
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

        let mut bus = self.mutex.lock_bus().await;
        let dom_random = self.rand.gen();

        let payload = BusDomPayload::DiscoverInitial {
            random: dom_random,
            min_wait_us: 1_000,
            max_wait_us: 10_000,
            offers: ManagedArcSlab::from_slice(&avail_addrs),
        };
        let message = BusDomMessage::new(
            RefAddr::local_dom_addr(),
            RefAddr::local_broadcast_addr(),
            payload,
        );
        bus.send_blocking(message).unwrap();

        // Start the receive
        let start = timer.get_ticks();
        let mut resps = Vec::<_, 32>::new();

        // Collect until timeout, or max messages received
        while !resps.is_full() {
            let maybe_msg =
                super::receive_timeout_micros::<T, R>(bus.deref_mut(), start, 20_000u32).await;

            if let Some(msg) = maybe_msg {
                resps.push(msg).map_err(drop)?;
            } else {
                break;
            }
        }

        println!("DOM RESPS: {:?}", resps);

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
                .inspect(|r| println!("START: {:?}", r))
                .filter_map(|resp| resp.validate_discover_ack_addr(dom_random).ok())
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
                bus.send_blocking(BusDomMessage::generate_discover_ack_ack(
                    *addr,
                    self.rand.gen(),
                    *sub_random,
                ))
                .unwrap();
            }
        }

        Ok(accepted)
    }
}
