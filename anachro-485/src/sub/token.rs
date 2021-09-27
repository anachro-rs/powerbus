use core::marker::PhantomData;

use byte_slab::BSlab;
use groundhog::RollingTimer;
use rand::Rng;

use crate::{async_sleep_micros, async_sleep_millis, dispatch::{Dispatch, DispatchSocket, LocalPacket, INVALID_OWN_ADDR}, dom::TOKEN_PORT, icd::{AddrPort, DomDiscoveryPayload, DomTokenGrantPayload, SLAB_SIZE, SubDiscoveryPayload, SubTokenReleasePayload, TOTAL_SLABS, VecAddr}, receive_timeout_micros, timing::{SUB_BROADACKACK_WAIT_US, SUB_INITIAL_DISCO_WAIT_US, SUB_PING_WAIT_US}};

pub struct Token<R, A>
where
    R: RollingTimer<Tick = u32> + Default,
    A: Rng,
{
    _timer: PhantomData<R>,
    dispatch: &'static Dispatch<8>,
    socket: DispatchSocket<'static>,
    rand: A,
    alloc: &'static BSlab<TOTAL_SLABS, SLAB_SIZE>,
    bad_ticks: u8,
}

impl<R, A> Token<R, A>
where
    R: RollingTimer<Tick = u32> + Default,
    A: Rng,
{
    pub fn new(
        rand: A,
        dispatch: &'static Dispatch<8>,
        socket: DispatchSocket<'static>,
        alloc: &'static BSlab<TOTAL_SLABS, SLAB_SIZE>,
    ) -> Self {
        Self {
            _timer: PhantomData,
            rand,
            socket,
            alloc,
            dispatch,
            bad_ticks: 0,
        }
    }

    pub async fn poll(&mut self) -> ! {
        loop {
            match self.poll_inner().await {
                Ok(_) => {},
                Err(_) => {
                    defmt::warn!("Bad sub token poll");
                }
            }
        }
    }

    pub async fn poll_inner(&mut self) -> Result<(), ()> {
        let timer = R::default();
        let addr = match self.dispatch.get_addr() {
            None => {
                async_sleep_millis::<R>(timer.get_ticks(), 10).await;
                return Ok(());
            }
            Some(addr) => addr,
        };


        let maybe_msg =
            receive_timeout_micros::<R, DomTokenGrantPayload>(&mut self.socket, timer.get_ticks(), 1_000_000)
                .await;

        let msg = match maybe_msg {
            Some(msg) => {
                self.bad_ticks = 0;
                msg
            },
            None => {
                defmt::warn!("No grant for a full second!");
                self.bad_ticks += 1;

                if self.bad_ticks >= 10 {
                    defmt::panic!("Too quiet!");
                }

                return Err(())
            },
        };

        let start = timer.get_ticks();
        self.socket.clear_empty()?;
        self.socket.auth_send()?;
        let duration = msg.body.max_time_us / 2;

        while timer.micros_since(start) <= duration {
            if self.socket.is_empty()? {
                break;
            } else {
                self.socket.auth_send()?;
                async_sleep_micros::<R>(timer.get_ticks(), duration / 4).await;
            }
        }

        let payload = SubTokenReleasePayload {
            random: msg.body.random,
        };

        let msg = LocalPacket::from_parts_with_alloc(
            payload,
            AddrPort::from_parts(VecAddr::from_local_addr(addr), TOKEN_PORT),
            AddrPort::from_parts(VecAddr::local_dom_addr(), TOKEN_PORT),
            None,
            self.alloc,
        )
        .ok_or(())?;

        self.socket.try_send_authd(msg).map_err(drop)?;

        Ok(())

        // Got a token, stick a message in the queue
    }

}
