use core::sync::atomic::{AtomicU32, Ordering::SeqCst};
use heapless::Vec;

pub mod discover;
pub mod token;

pub struct AddrTable32 {
    active: AtomicU32,
}

impl AddrTable32 {
    pub const fn new() -> Self {
        Self {
            active: AtomicU32::new(0x0000_0000),
        }
    }

    pub fn get_active_addrs(&self) -> Vec<u8, 32> {
        let mut copy = self.active.load(SeqCst);
        let mut ret = Vec::new();

        for i in 1..=32 {
            if copy & 0x0000_0001 != 0 {
                ret.push(i).ok();
            }
            copy >>= 1;
        }

        ret
    }

    pub fn get_available_addrs(&self) -> Vec<u8, 32> {
        let mut copy = !self.active.load(SeqCst);
        let mut ret = Vec::new();

        for i in 1..=32 {
            if copy & 0x0000_0001 != 0 {
                ret.push(i).ok();
            }
            copy >>= 1;
        }

        ret
    }

    pub fn commit_reserved_addr(&self, addr: u8) -> Result<(), ()> {
        if addr > 32 || addr == 0 {
            return Err(());
        }

        let old = self.active.fetch_or(1 << (addr - 1), SeqCst);

        if old & (1 << (addr - 1)) != 0 {
            Err(())
        } else {
            Ok(())
        }
    }

    pub fn release_active_addr(&self, addr: u8) -> Result<(), ()> {
        if addr > 32 || addr == 0 {
            return Err(());
        }

        let old = self.active.fetch_and(!(1 << (addr - 1)), SeqCst);

        if old & (1 << (addr - 1)) == 0 {
            Err(())
        } else {
            Ok(())
        }
    }
}

pub const NUM_PORTS: usize = 8;
pub const DISCOVERY_PORT: u16 = 10;
pub const TOKEN_PORT: u16 = 20;

#[cfg(TODO)]
mod todo {
    // TODO: This is a WIP to figure out how I can more easily
    // define all the moving parts of a dom/sub using macros.
    //
    // Right now, this requires declaring a bunch of pinned and
    // cassette'd futures, and polling them in a sort of round
    // robin'd

    use crate::dispatch::{Dispatch, DispatchSocket};
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
            let port = dispatch.register_port(DISCOVERY_PORT)?;

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
}
