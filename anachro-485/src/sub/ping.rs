use core::marker::PhantomData;
use std::ops::DerefMut;
use groundhog::RollingTimer;
use rand::Rng;
use crate::{async_sleep_millis, sub::{SubInterface, AsyncSubMutex}, icd::{BusDomMessage, BusDomPayload, RefAddr}};


pub struct Ping<R, T, A>
where
    R: RollingTimer<Tick = u32> + Default,
    T: SubInterface,
    A: Rng,
{
    _timer: PhantomData<R>,
    mutex: AsyncSubMutex<T>,
    rand: A,
}
