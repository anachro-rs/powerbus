use std::{sync::{Arc, Mutex, MutexGuard}, task::Poll};
use crate::icd::{BusDomMessage, BusSubMessage};
use futures::future::poll_fn;
use groundhog::RollingTimer;

pub mod discover;
pub mod ping;

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
