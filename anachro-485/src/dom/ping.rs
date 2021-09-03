use crate::{
    async_sleep_millis,
    dom::{AsyncDomMutex, DomInterface},
    icd::{BusDomMessage, BusDomPayload, VecAddr},
};
use core::{marker::PhantomData, ops::DerefMut};
use groundhog::RollingTimer;
use rand::Rng;

pub struct Ping<R, T, A>
where
    R: RollingTimer<Tick = u32> + Default,
    T: DomInterface,
    A: Rng,
{
    _timer: PhantomData<R>,
    mutex: AsyncDomMutex<T>,
    rand: A,
}

impl<R, T, A> Ping<R, T, A>
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
            let active = {
                let table = self.mutex.lock_table().await;
                table.get_active_addrs()
            };
            if active.is_empty() {
                continue;
            }
            let choice = match active.get(self.rand.gen_range(0..active.len())) {
                Some(addr) => *addr,
                None => continue,
            };

            let mut bus = self.mutex.lock_bus().await;
            let dom_random = self.rand.gen();

            let payload = BusDomPayload::PingReq {
                random: dom_random,
                min_wait_us: 0,
                max_wait_us: 0,
            };
            let msg = BusDomMessage {
                src: VecAddr::local_dom_addr(),
                dst: VecAddr::from_local_addr(choice),
                payload,
            };
            match bus.send_blocking(msg) {
                Ok(_) => {}
                Err(_) => continue,
            }

            let start = timer.get_ticks();

            let got = 'inner: loop {
                let maybe_msg =
                    super::receive_timeout_micros::<T, R>(bus.deref_mut(), start, 10_000u32).await;

                let msg = match maybe_msg {
                    Some(msg) => msg,
                    None => break 'inner false,
                };

                if msg.validate_ping_ack(dom_random).is_ok() {
                    break 'inner true;
                }
            };

            if got {
                println!("{} say hi", choice);
            } else {
                println!("{} says *crickets*", choice);
            }
        }
    }
}
