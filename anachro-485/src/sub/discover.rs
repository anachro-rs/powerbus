use core::marker::PhantomData;

use byte_slab::BSlab;
use groundhog::RollingTimer;
use rand::Rng;

use crate::{
    async_sleep_micros,
    dispatch::{Dispatch, DispatchSocket, LocalPacket, INVALID_OWN_ADDR},
    icd::{BusDomPayload, BusSubPayload, SLAB_SIZE, TOTAL_SLABS},
    receive_timeout_micros,
};

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
                println!("Addr obtained! {}", addr);
                return Ok(addr);
            } else {
                println!("Sub poll good, still working...");
            }
        }
    }

    pub async fn obtain_addr_inner(&mut self) -> Result<Option<u8>, ()> {
        let timer = R::default();
        self.dispatch.set_addr(INVALID_OWN_ADDR);

        let msg = match receive_timeout_micros::<R, BusDomPayload>(
            &mut self.socket,
            timer.get_ticks(),
            1_000_000,
        )
        .await
        {
            Some(msg) => msg,
            None => return Ok(None),
        };

        let (addr, sub_random, delay, resp) = if let Some((addr, sub_random, delay, resp)) =
            BusSubPayload::generate_discover_ack(&mut self.rand, msg.body, &msg.hdr)
        {
            (addr, sub_random, delay, resp)
        } else {
            return Ok(None);
        };

        // TODO: Move the "response percentage" to the dom, let it respond
        // to collisions/etc to decrease
        if self.rand.gen_range(0..4) != 0 {
            return Ok(None);
        }

        // Set our own addr to the provisionally chosen one
        self.dispatch.set_addr(addr);

        async_sleep_micros::<R>(timer.get_ticks(), delay).await;

        let msg =
            LocalPacket::from_parts_with_alloc(resp.body, resp.hdr.src, resp.hdr.dst, self.alloc)
                .ok_or(())?;
        self.socket.try_send(msg).map_err(drop)?;

        let start = timer.get_ticks();
        loop {
            let msg =
                match receive_timeout_micros::<R, BusDomPayload>(&mut self.socket, start, 250_000)
                    .await
                {
                    Some(msg) => msg,
                    None => return Ok(None),
                };

            match msg.body.validate_discover_ack_ack(&msg.hdr, sub_random) {
                Ok(new_addr) if new_addr == addr => {
                    println!("yey");
                    break;
                }
                Ok(_) => {
                    println!("wtf?");
                    return Ok(None);
                }
                Err(_) => {
                    println!("ohno");
                    return Ok(None);
                }
            }
        }

        let start = timer.get_ticks();
        let mut success_ct: u8 = 0;

        loop {
            let msg = match receive_timeout_micros::<R, BusDomPayload>(
                &mut self.socket,
                start,
                5_000_000,
            )
            .await
            {
                Some(msg) => msg,
                None => return Ok(None),
            };

            let j_start = timer.get_ticks();
            if let Some((jitter, resp)) =
                BusSubPayload::generate_ping_ack(&mut self.rand, addr, msg.body, &msg.hdr)
            {
                async_sleep_micros::<R>(j_start, jitter).await;

                let msg = LocalPacket::from_parts_with_alloc(
                    resp.body,
                    resp.hdr.src,
                    resp.hdr.dst,
                    self.alloc,
                )
                .ok_or(())?;
                self.socket.try_send(msg).map_err(drop)?;

                success_ct += 1;
                if success_ct >= 2 {
                    return Ok(Some(addr));
                }
            } else {
                return Ok(None);
            }
        }
    }
}
