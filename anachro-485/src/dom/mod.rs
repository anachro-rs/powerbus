use crate::dispatch::{Dispatch, DispatchSocket};

// TODO: `no_std`

use heapless::Vec;

pub mod discover;

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

pub const NUM_PORTS: usize = 8;
pub const MANAGEMENT_PORT: u16 = 10;
use cassette::Cassette;

pub struct DomHandle<T>
where
    T: core::future::Future + Unpin,
{
    dispatch: &'static Dispatch<NUM_PORTS>,
    bus_mgmt_port: DispatchSocket<'static>,
    boo: Cassette<T>,
}

impl<T> DomHandle<T>
where
    T: core::future::Future + Unpin,
{
    pub fn new(dispatch: &'static Dispatch<NUM_PORTS>, b: T) -> Option<Self> {
        let port = dispatch.register_port(MANAGEMENT_PORT)?;

        Some(Self {
            dispatch,
            bus_mgmt_port: port,
            boo: Cassette::new(b),
        })
    }

    pub fn poll(&mut self) {
        self.boo.poll_on();
    }
}

pub async fn weeew() -> Result<u8, ()> {
    Ok(42)
}

#[macro_export]
macro_rules! declare_dom {
    ({
        name: $name:ident,
        dispatch: $dispatch:ident,
    }) => {
        let test_fut = $crate::dom::weeew();
        pin_mut!(test_fut);
        let mut $name = $crate::dom::DomHandle::new(&$dispatch, test_fut).unwrap();
    };
}
