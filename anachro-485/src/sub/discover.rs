use core::marker::PhantomData;

use byte_slab::BSlab;
use groundhog::RollingTimer;
use rand::Rng;

use crate::{async_sleep_micros, dispatch::{Dispatch, DispatchSocket, LocalPacket, INVALID_OWN_ADDR}, icd::{DomDiscoveryPayload, SubDiscoveryPayload, SLAB_SIZE, TOTAL_SLABS}, receive_timeout_micros, timing::{SUB_BROADACKACK_WAIT_US, SUB_INITIAL_DISCO_WAIT_US, SUB_PING_WAIT_US}};

pub struct Discovery<R, A>
where
    R: RollingTimer<Tick = u32> + Default,
    A: Rng,
{
    _timer: PhantomData<R>,
    dispatch: &'static Dispatch<8>,
    socket: DispatchSocket<'static>,
    rand: A,
    alloc: &'static BSlab<TOTAL_SLABS, SLAB_SIZE>,
}

impl<R, A> Discovery<R, A>
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
        }
    }

    pub async fn obtain_addr(&mut self) -> Result<u8, ()> {
        loop {
            if let Some(addr) = self.obtain_addr_inner().await? {
                // println!("Addr obtained! {}", addr);
                return Ok(addr);
            } else {
                // println!("Sub poll good, still working...");
            }
        }
    }

    pub async fn obtain_addr_inner(&mut self) -> Result<Option<u8>, ()> {
        defmt::info!("Sub start discovery...");
        let timer = R::default();

        self.socket.auth_flush().ok();
        async_sleep_micros::<R>(timer.get_ticks(), 2_000).await;

        self.dispatch.set_addr(INVALID_OWN_ADDR);

        let msg = match receive_timeout_micros::<R, DomDiscoveryPayload>(
            &mut self.socket,
            timer.get_ticks(),
            SUB_INITIAL_DISCO_WAIT_US,
        )
        .await
        {
            Some(msg) => msg,
            None => return Ok(None),
        };

        let (addr, sub_random, delay, max_delay, resp) = if let Some((addr, sub_random, delay, max_delay, resp)) =
            SubDiscoveryPayload::generate_discover_ack(&mut self.rand, msg.body, &msg.hdr)
        {
            (addr, sub_random, delay, max_delay, resp)
        } else {
            return Ok(None);
        };

        // TODO: Move the "response percentage" to the dom, let it respond
        // to collisions/etc to decrease
        // if self.rand.gen_range(0..4) != 0 {
        //     return Ok(None);
        // }

        defmt::info!("Sub got initial...");

        // Set our own addr to the provisionally chosen one
        self.dispatch.set_addr(addr);
        defmt::info!("Set addr to {=u8}", addr);

        let start_sleep = timer.get_ticks();

        async_sleep_micros::<R>(start_sleep, delay).await;

        defmt::info!("Sending broadack");
        let msg =
            LocalPacket::from_parts_with_alloc(resp.body, resp.hdr.src, resp.hdr.dst, None, self.alloc)
                .ok_or(())?;
        self.socket.try_send_authd(msg).map_err(drop)?;

        defmt::assert!(max_delay >= delay);
        let remaining_sleep = max_delay - delay;

        let start = timer.get_ticks();
        loop {
            let msg =
                match receive_timeout_micros::<R, DomDiscoveryPayload>(&mut self.socket, start, SUB_BROADACKACK_WAIT_US + remaining_sleep)
                    .await
                {
                    Some(msg) => msg,
                    None => {
                        defmt::warn!("Sub Timeout 1");
                        return Ok(None)
                    },
                };

            match msg.body.validate_discover_ack_ack(&msg.hdr, sub_random) {
                Ok(new_addr) if new_addr == addr => {
                    // println!("yey");
                    defmt::info!("good ackack!");
                    break;
                }
                Ok(_) => {
                    // println!("wtf?");
                    // return Ok(None);
                    defmt::warn!("OK message not for us");
                }
                Err(_) => {
                    // println!("ohno");
                    // return Ok(None);
                    defmt::warn!("Bad Message");
                }
            }
        }

        let mut success_ct: u8 = 0;
        defmt::info!("Sub got next...");


        loop {
            defmt::info!("Sub got loop {=u8}...", success_ct);
            let start = timer.get_ticks();
            let msg = match receive_timeout_micros::<R, DomDiscoveryPayload>(
                &mut self.socket,
                start,
                SUB_PING_WAIT_US,
            )
            .await
            {
                Some(msg) => msg,
                None => {
                    defmt::warn!("Timeout 2");
                    return Ok(None);
                },
            };

            let j_start = timer.get_ticks();
            if let Some((jitter, resp)) =
                SubDiscoveryPayload::generate_ping_ack(&mut self.rand, addr, msg.body, &msg.hdr)
            {
                async_sleep_micros::<R>(j_start, jitter).await;

                let msg = LocalPacket::from_parts_with_alloc(
                    resp.body,
                    resp.hdr.src,
                    resp.hdr.dst,
                    None,
                    self.alloc,
                )
                .ok_or(())?;
                self.socket.try_send_authd(msg).map_err(drop)?;

                success_ct += 1;
                if success_ct >= 2 {
                    defmt::info!("Sub got yeyeyeye...");
                    return Ok(Some(addr));
                }
            } else {
                defmt::error!("Bad generate_ping_ack!");
                return Ok(None);
            }
        }
    }
}
