#![allow(unused_imports)]

use crate::{
    icd::{BusDomMessage, BusDomPayload, VecAddr},
    sub::{AsyncSubMutex, SubInterface},
};
use core::marker::PhantomData;
use groundhog::RollingTimer;
use rand::Rng;
use std::ops::DerefMut;

pub struct Ping<R, T, A>
where
    R: RollingTimer<Tick = u32> + Default,
    T: SubInterface,
    A: Rng,
{
    _timer: PhantomData<R>,
    _mutex: AsyncSubMutex<T>,
    _rand: A,
}
