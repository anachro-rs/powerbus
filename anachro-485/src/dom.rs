pub mod discover {
    use byte_slab::ManagedArcSlab;
    use groundhog::RollingTimer;
    use core::marker::PhantomData;
    use crate::async_sleep_millis;
    use crate::icd::BusSubMessage;
    use crate::icd::RefAddr;
    use super::AsyncDomMutex;
    use super::DomInterface;
    use rand::Rng;
    use crate::icd::{BusDomMessage, BusDomPayload};
    use heapless::{Vec, FnvIndexSet, FnvIndexMap};
    use core::ops::DerefMut;
    use core::iter::FromIterator;

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
                    Ok(_) => println!("Poll good!"),
                    Err(_) => println!("Poll bad!"),
                }
            }
        }

        pub async fn poll_inner(&mut self) -> Result<(), ()> {
            let avail_addrs = {
                self.mutex
                    .lock_table()
                    .await
                    .reserve_all_addrs()
            };

            if avail_addrs.is_empty() {
                return Err(());
            }


            // Broadcast initial
            let readies = self.broadcast_initial(&avail_addrs).await?;
            println!("READIES: {:?}", readies);
            // TODO!

            Ok(())
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
                let maybe_msg = super::receive_timeout_micros::<T, R>(bus.deref_mut(), start, 12_000u32).await;

                if let Some(msg) = maybe_msg {
                    resps.push(msg).map_err(drop)?;
                } else {
                    break;
                }
            }

            let mut offered = FnvIndexSet::<u8, 32>::new();
            let mut seen = FnvIndexSet::<u8, 32>::new();
            let mut dupes = FnvIndexSet::<u8, 32>::new();

            avail_addrs.iter().try_for_each::<_, Result<_, u8>>(|a| {
                offered.insert(*a)?;
                Ok(())
            }).map_err(drop)?;

            let mut response_pairs = FnvIndexMap::<_, _, 32>::from_iter(
                resps.iter()
                    // Remove any items that don't check out
                    .filter_map(|resp| resp.validate_discover_ack_addr(dom_random).ok())
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
                    .filter_map(Result::<_, u8>::ok)
            );

            // Remove any duplicates that have been seen
            dupes.iter().for_each(|d| {
                let _ = response_pairs.remove(d);
            });

            let mut accepted = Vec::<u8, 32>::new();
            // ACK acceptable response pairs
            for (addr, sub_random) in response_pairs.iter() {
                if let Ok(_) = accepted.push(*addr) {
                    bus.send_blocking(
                        BusDomMessage::generate_discover_ack_ack(
                            *addr,
                            self.rand.gen(),
                            *sub_random
                        )
                    ).unwrap();
                }
            }


            Ok(accepted)
        }
    }
}

use crate::icd::{BusDomMessage, BusSubMessage};
use std::sync::{Arc, Mutex, MutexGuard};
use core::task::Poll;
use futures::future::poll_fn;
use groundhog::RollingTimer;

pub trait DomInterface {
    fn send_blocking(&mut self, msg: BusDomMessage) -> Result<(), BusDomMessage>;
    fn pop(&mut self) -> Option<BusSubMessage<'static>>;
}

// hmmm
// This will need to look way different in no-std
#[derive(Clone)]
pub struct AsyncDomMutex<T>
where
    T: DomInterface,
{
    bus: Arc<Mutex<T>>,
    table: Arc<Mutex<AddrTable32>>,
}

impl<T> AsyncDomMutex<T>
where
    T: DomInterface,
{
    pub fn new(intfc: T) -> Self {
        Self {
            bus: Arc::new(Mutex::new(intfc)),
            table: Arc::new(Mutex::new(AddrTable32::new())),
        }
    }

    // TODO: Custom type also with DerefMut
    pub async fn lock_bus(&self) -> MutexGuard<'_, T> {
        poll_fn(|_| {
            match self.bus.try_lock() {
                Ok(mg) => Poll::Ready(mg),
                Err(_) => Poll::Pending
            }
        }).await
    }

    // TODO: Custom type also with DerefMut
    pub async fn lock_table(&self) -> MutexGuard<'_, AddrTable32> {
        poll_fn(|_| {
            match self.table.try_lock() {
                Ok(mg) => Poll::Ready(mg),
                Err(_) => Poll::Pending
            }
        }).await
    }
}

pub async fn receive_timeout_micros<T, R>(
    interface: &mut T,
    start: R::Tick,
    duration: R::Tick,
) -> Option<BusSubMessage<'static>>
where
    T: DomInterface,
    R: RollingTimer<Tick = u32> + Default,
{
    poll_fn(move |_| {
        let timer = R::default();
        if timer.micros_since(start) >= duration {
            Poll::Ready(None)
        } else {
            match interface.pop() {
                m @ Some(_) => Poll::Ready(m),
                _ => Poll::Pending
            }
        }
    }).await
}

use heapless::Vec;

pub struct AddrTable32 {
    available: u32,
    reserved: u32,
    active: u32,
}

impl AddrTable32 {
    pub fn new() -> Self {
        Self {
            available: 0xFFFF_FFFF,
            reserved: 0x0000_0000,
            active: 0x0000_0000,
        }
    }

    pub fn get_reserved_addrs(&mut self) -> Vec<u8, 32> {
        let mut copy = self.reserved;
        let mut ret = Vec::new();

        for i in 1..=32 {
            if copy & 0x0000_0001 != 0 {
                ret.push(i).ok();
            }
            copy >>= 1;
        }

        ret
    }

    /// Returns any reserved addrs
    pub fn reserve_all_addrs(&mut self) -> Vec<u8, 32> {
        self.reserved |= self.available;
        self.available = 0;
        let mut ret = Vec::new();
        let mut copy = self.reserved;

        for i in 1..=32 {
            if copy & 0x0000_0001 != 0 {
                ret.push(i).ok();
            }
            copy >>= 1;
        }

        ret
    }

    pub fn reserve_addr(&mut self) -> Option<u8> {
        match self.available.trailing_zeros() {
            tz @ 0..=31 => {
                let mask = 1 << tz;
                assert!((self.reserved & mask) == 0);

                self.reserved |= mask;
                Some((tz + 1) as u8)
            },
            _ => None,
        }
    }

    pub fn release_reserved_addr(&mut self, addr: u8) -> Result<(), ()> {
        if addr > 32 || addr == 0 {
            return Err(());
        }

        let mask = 1 << (addr - 1);

        if (self.reserved & mask) != 0 {
            return Err(());
        }

        self.reserved &= !mask;
        self.available |= mask;

        Ok(())
    }

    pub fn commit_reserved_addr(&mut self, addr: u8) -> Result<(), ()> {
        if addr > 32 || addr == 0 {
            return Err(());
        }

        let mask = 1 << (addr - 1);

        if (self.reserved & mask) != 0 {
            return Err(());
        }

        self.reserved &= !mask;
        self.active |= mask;

        Ok(())
    }

    pub fn release_active_addr(&mut self, addr: u8) -> Result<(), ()> {
        if addr > 32 || addr == 0 {
            return Err(());
        }

        let mask = 1 << (addr - 1);

        if (self.active & mask) != 0 {
            return Err(());
        }

        self.active &= !mask;
        self.available |= mask;

        Ok(())
    }
}

