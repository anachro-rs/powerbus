use serde::{Serialize, Deserialize};
use std::marker::PhantomData;

#[derive(Debug, Serialize, Deserialize)]
pub struct BusDomMessage<'a> {
    // Mark the 'a lifetime as borrowed
    #[serde(borrow)]
    _lt_a: PhantomData<&'a ()>,

    src: RefAddr<'a>,
    dst: RefAddr<'a>,
    payload: BusDomPayload<'a>,
}

impl<'a> BusDomMessage<'a> {
    pub fn new(src: RefAddr<'a>, dst: RefAddr<'a>, payload: BusDomPayload<'a>) -> Self {
        Self {
            src,
            dst,
            payload,
            _lt_a: PhantomData,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BusSubMessage<'a> {
    // Mark the 'a lifetime as borrowed
    #[serde(borrow)]
    _lt_a: PhantomData<&'a ()>,

    src: RefAddr<'a>,
    dst: RefAddr<'a>,
    payload: BusSubPayload<'a>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RefAddr<'a> {
    bytes: &'a [u8],
}

impl<'a> RefAddr<'a> {
    pub fn from_addrs(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
        }
    }

    pub const LOCAL_DOM_ADDR: RefAddr<'static> = RefAddr { bytes: &[0x00] };
    pub const LOCAL_BROADCAST_ADDR: RefAddr<'static> = RefAddr { bytes: &[0xFF] };
}

#[derive(Debug, Serialize, Deserialize)]
pub enum BusDomPayload<'a> {
    ResetConnection,
    Opaque(&'a [u8]),
    DiscoverInitial {
        random: u32,
        min_wait_us: u32,
        max_wait_us: u32,
        offers: &'a [u8],
    },
    DiscoverAckAck {
        own_id: u8,
        own_random: u32,
        own_id_ownrand_checksum: u32,
    },
    BusGrant {
        tx_bytes_ready: u32,
        rx_bytes_avail: u32,
        max_grant_us: u32,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub enum BusSubPayload<'a> {
    Opaque(&'a [u8]),
    DiscoverAck {
        own_id: u8,
        own_id_rand_checksum: u32,
        own_random: u32,
    },
    BusGrantAccept {
        tx_bytes_ready: u32,
        rx_bytes_avail: u32,
    },
    BusGrantRelease,
}
