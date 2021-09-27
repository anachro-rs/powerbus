// Okay, what do we have here. Let's get back to layers:

#![cfg_attr(not(any(test, feature = "std")), no_std)]

pub mod dispatch;
pub mod dom;
pub mod icd;
pub mod sub;
pub mod timing;

use dispatch::{DispatchSocket, LocalHeader};
use groundhog::{self, RollingTimer};
use postcard::from_bytes;
use serde::de::DeserializeOwned;

/*


* Routing level
    * Source: slice of bytes
    * Dest: slice of bytes
    * Payload: bag o bytes
* Lowest level, cobs framed data

*/

use core::{ops::Deref, task::Poll};
use futures::future::poll_fn;

// TODO: This should probably live in groundhog
pub async fn async_sleep_millis<R>(start: R::Tick, millis: R::Tick)
where
    R: RollingTimer + Default,
    R::Tick: PartialOrd,
{
    poll_fn(|_| {
        let timer = R::default();
        if timer.millis_since(start) >= millis {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    })
    .await;
}

// TODO: This should probably live in groundhog
pub async fn async_sleep_micros<R>(start: R::Tick, millis: R::Tick)
where
    R: RollingTimer + Default,
    R::Tick: PartialOrd,
{
    poll_fn(|_| {
        let timer = R::default();
        if timer.micros_since(start) >= millis {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    })
    .await;
}

pub struct HeaderPacket<T> {
    pub hdr: LocalHeader,
    pub body: T,
}

pub async fn receive_timeout_micros<R, T>(
    interface: &mut DispatchSocket<'static>,
    start: R::Tick,
    duration: R::Tick,
) -> Option<HeaderPacket<T>>
where
    R: RollingTimer<Tick = u32> + Default,
    T: DeserializeOwned,
{
    poll_fn(move |_| {
        let timer = R::default();
        if timer.micros_since(start) >= duration {
            Poll::Ready(None)
        } else {
            match interface.try_recv() {
                Some(msg) => match from_bytes(msg.payload.deref()) {
                    Ok(m) => Poll::Ready(Some(HeaderPacket {
                        hdr: msg.hdr,
                        body: m,
                    })),
                    Err(_) => {
                        defmt::warn!("Bad deser!");
                        Poll::Pending
                    },
                },
                _ => Poll::Pending,
            }
        }
    })
    .await
}
