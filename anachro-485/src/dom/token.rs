use crate::{async_sleep_micros, async_sleep_millis, dispatch::{DispatchSocket, LocalPacket}, icd::{
        AddrPort, DomDiscoveryPayload, DomTokenGrantPayload, SubDiscoveryPayload,
        SubTokenReleasePayload, VecAddr, SLAB_SIZE, TOTAL_SLABS,
    }, receive_timeout_micros, timing::{
        DOM_BROADCAST_MAX_WAIT_US, DOM_BROADCAST_MIN_WAIT_US, DOM_PING_MAX_WAIT_US,
        DOM_PING_MIN_WAIT_US,
    }};

use core::{iter::FromIterator, marker::PhantomData, ops::Deref};

use byte_slab::BSlab;
use groundhog::RollingTimer;
use heapless::{FnvIndexMap, FnvIndexSet, Vec};
use rand::Rng;

use crate::dom::AddrTable32;

use super::TOKEN_PORT;

pub struct Token<R, A>
where
    R: RollingTimer<Tick = u32> + Default,
    A: Rng,
{
    _timer: PhantomData<R>,
    socket: DispatchSocket<'static>,
    table: &'static AddrTable32,
    rand: A,
    alloc: &'static BSlab<TOTAL_SLABS, SLAB_SIZE>,
    ping_table: [Option<u32>; 32],
}

impl<R, A> Token<R, A>
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
            alloc,
            ping_table: [None; 32],
        }
    }

    pub async fn poll(&mut self) -> ! {
        loop {
            match self.poll_inner().await {
                Ok(_) => {}
                Err(_) => {
                    defmt::warn!("Bad tokening!");
                }
            }
        }
    }

    pub async fn poll_inner(&mut self) -> Result<(), ()> {
        let active_addrs = self.table.get_active_addrs();
        let timer = R::default();

        if active_addrs.is_empty() {
            async_sleep_millis::<R>(timer.get_ticks(), 100).await;
            return Ok(());
        }

        let mut last_start = timer.get_ticks();

        for addr in active_addrs {
            if self.ping_table[addr as usize].is_none() {
                self.ping_table[addr as usize] = Some(timer.get_ticks());
            }

            defmt::info!("Querying {=u8}...", addr);
            let random = self.rand.gen();
            let addr_port = AddrPort::from_parts(VecAddr::from_local_addr(addr), TOKEN_PORT);

            let payload = DomTokenGrantPayload {
                random,
                max_time_us: 50_000,
            };

            let msg = LocalPacket::from_parts_with_alloc(
                payload,
                AddrPort::from_parts(VecAddr::local_dom_addr(), TOKEN_PORT),
                addr_port.clone(),
                Some(50_000),
                self.alloc,
            )
            .ok_or(())?;

            self.socket.try_send_authd(msg).map_err(drop)?;
            let start = timer.get_ticks();

            'inner: loop {
                let maybe_msg = receive_timeout_micros::<R, SubTokenReleasePayload>(
                    &mut self.socket,
                    start,
                    50_000,
                )
                .await;

                let msg = match maybe_msg {
                    Some(msg) => msg,
                    None => {
                        defmt::warn!("No response from {=u8}!", addr);
                        break 'inner;
                    }
                };

                let good_src = msg.hdr.src == addr_port;
                let good_rnd = msg.body.random == random;

                if good_rnd && good_src {
                    self.ping_table[addr as usize] = Some(timer.get_ticks());
                    break 'inner;
                }
            }

            // We *may* have gotten a message this time, but let's check if
            // our device is at timeout
            let mut bad = false;
            if let Some(time) = self.ping_table[addr as usize] {
                bad = timer.millis_since(time) >= 5_000;
            }
            if bad {
                self.ping_table[addr as usize] = None;
                defmt::warn!("Yeeting {=u8}", addr);
                self.table.release_active_addr(addr)?;
            }

            if timer.micros_since(last_start) <= 1000 {
                async_sleep_micros::<R>(last_start, 1000).await;
            }
            last_start = timer.get_ticks();
        }

        async_sleep_millis::<R>(last_start, 10).await;

        Ok(())
    }
}
