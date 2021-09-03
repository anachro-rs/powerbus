// Okay, what do we have here. Let's get back to layers:

pub mod dom;
pub mod icd;
pub mod sub;
pub mod dispatch;

use groundhog::{self, RollingTimer};

/*


* Routing level
    * Source: slice of bytes
    * Dest: slice of bytes
    * Payload: bag o bytes
* Lowest level, cobs framed data

*/

use core::task::Poll;
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

