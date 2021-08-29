use std::{sync::{Arc, Mutex, MutexGuard}, task::Poll};
use crate::icd::{BusDomMessage, BusSubMessage};
use futures::future::poll_fn;
use groundhog::RollingTimer;

pub mod discover {
    use std::{marker::PhantomData, ops::DerefMut};

    use groundhog::RollingTimer;
    use rand::Rng;

    use crate::async_sleep_millis;

    use super::{AsyncSubMutex, SubInterface};

    pub struct Discovery<R, T, A>
    where
        R: RollingTimer<Tick = u32> + Default,
        T: SubInterface,
        A: Rng,
    {
        _timer: PhantomData<R>,
        mutex: AsyncSubMutex<T>,
        rand: A,
    }


    impl<R, T, A> Discovery<R, T, A>
    where
        R: RollingTimer<Tick = u32> + Default,
        T: SubInterface,
        A: Rng,
    {
        pub fn new(mutex: AsyncSubMutex<T>, rand: A) -> Self {
            Self {
                _timer: PhantomData,
                mutex,
                rand,
            }
        }

        pub async fn obtain_addr(&mut self) -> Result<u8, ()> {
            let timer = R::default();
            loop {
                async_sleep_millis::<R>(timer.get_ticks(), 1000u32).await;

                if let Some(addr) = self.poll_inner().await? {
                    println!("Addr obtained! {}", addr);
                    return Ok(addr);
                } else {
                    println!("Sub poll good, still working...");
                }
            }
        }

        pub async fn poll_inner(&mut self) -> Result<Option<u8>, ()> {
            let mut bus = self.mutex.lock_bus().await;
            let timer = R::default();

            let msg = super::receive_timeout_micros::<T, R>(bus.deref_mut(), timer.get_ticks(), 1000).await;
            println!("SUB GOT MSG: {:?}", msg);
            Ok(None)
        }
    }
}

pub trait SubInterface {
    fn send_blocking<'a>(&mut self, msg: BusSubMessage<'a>) -> Result<(), BusSubMessage<'a>>;
    fn pop(&mut self) -> Option<BusDomMessage<'static>>;
}

// hmmm
// This will need to look way different in no-std
#[derive(Clone)]
pub struct AsyncSubMutex<T>
where
    T: SubInterface,
{
    bus: Arc<Mutex<T>>,
}

impl<T> AsyncSubMutex<T>
where
    T: SubInterface,
{
    pub fn new(intfc: T) -> Self {
        Self {
            bus: Arc::new(Mutex::new(intfc)),
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
}

pub async fn receive_timeout_micros<T, R>(
    interface: &mut T,
    start: R::Tick,
    duration: R::Tick,
) -> Option<BusDomMessage<'static>>
where
    T: SubInterface,
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
