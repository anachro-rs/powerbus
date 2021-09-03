use crate::icd::{BusDomMessage, BusSubMessage};

// TODO: `no_std`
use std::sync::Arc;

use core::task::Poll;

use futures::future::poll_fn;
use groundhog::RollingTimer;
use heapless::Vec;
use spin::{Mutex, MutexGuard};

pub mod discover;
pub mod ping;

pub trait DomInterface {
    fn send_blocking<'a>(&mut self, msg: BusDomMessage<'a>) -> Result<(), BusDomMessage<'a>>;
    fn pop(&mut self) -> Option<BusSubMessage<'static>>;
}

// hmmm
// This will need to look way different in no-std
pub struct AsyncDomMutex<T>
where
    T: DomInterface,
{
    bus: Arc<Mutex<T>>,
    table: Arc<Mutex<AddrTable32>>,
}

impl<T> Clone for AsyncDomMutex<T>
where
    T: DomInterface,
{
    fn clone(&self) -> Self {
        Self {
            bus: self.bus.clone(),
            table: self.table.clone(),
        }
    }
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
        poll_fn(|_| match self.bus.try_lock() {
            Some(mg) => Poll::Ready(mg),
            None => Poll::Pending,
        })
        .await
    }

    // TODO: Custom type also with DerefMut
    pub async fn lock_table(&self) -> MutexGuard<'_, AddrTable32> {
        poll_fn(|_| match self.table.try_lock() {
            Some(mg) => Poll::Ready(mg),
            None => Poll::Pending,
        })
        .await
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
                _ => Poll::Pending,
            }
        }
    })
    .await
}

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

    pub fn get_reserved_addrs(&self) -> Vec<u8, 32> {
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

    pub fn get_active_addrs(&self) -> Vec<u8, 32> {
        let mut copy = self.active;
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
            }
            _ => None,
        }
    }

    pub fn release_reserved_addr(&mut self, addr: u8) -> Result<(), ()> {
        if addr > 32 || addr == 0 {
            return Err(());
        }

        let mask = 1 << (addr - 1);

        if (self.reserved & mask) == 0 {
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
        if (self.reserved & mask) == 0 {
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
