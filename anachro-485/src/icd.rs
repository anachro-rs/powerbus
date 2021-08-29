pub use byte_slab::ManagedArcSlab;
use byte_slab::SlabArc;
pub use heapless::Vec;
use serde::{Deserialize, Serialize};

pub const MAX_ADDR_SEGMENTS: usize = 8;

// These should prooooobably be configurable
pub const TOTAL_SLABS: usize = 128;
pub const SLAB_SIZE: usize = 512;

#[derive(Debug, Serialize, Deserialize)]
pub struct BusDomMessage<'a> {
    src: RefAddr,
    dst: RefAddr,

    #[serde(borrow)]
    payload: BusDomPayload<'a>,
}

impl<'a> BusDomMessage<'a> {
    pub fn new(src: RefAddr, dst: RefAddr, payload: BusDomPayload<'a>) -> Self {
        Self { src, dst, payload }
    }

    pub fn reroot(self, arc: &SlabArc<TOTAL_SLABS, SLAB_SIZE>) -> Option<BusDomMessage<'static>> {
        let BusDomMessage { src, dst, payload } = self;

        // See https://github.com/rust-lang/rust/issues/88423 for why we need to
        // be so verbose here.
        let payload: BusDomPayload<'static> = match payload {
            BusDomPayload::ResetConnection => BusDomPayload::ResetConnection,
            BusDomPayload::Opaque(p) => BusDomPayload::Opaque(p.reroot(arc)?),
            BusDomPayload::DiscoverInitial {
                random,
                min_wait_us,
                max_wait_us,
                offers,
            } => {
                let offers = offers.reroot(arc)?;
                BusDomPayload::DiscoverInitial {
                    random,
                    min_wait_us,
                    max_wait_us,
                    offers,
                }
            }
            BusDomPayload::DiscoverAckAck {
                own_id,
                own_random,
                own_id_ownrand_checksum,
            } => BusDomPayload::DiscoverAckAck {
                own_id,
                own_random,
                own_id_ownrand_checksum,
            },
            BusDomPayload::BusGrant {
                tx_bytes_ready,
                rx_bytes_avail,
                max_grant_us,
            } => BusDomPayload::BusGrant {
                tx_bytes_ready,
                rx_bytes_avail,
                max_grant_us,
            },
        };

        Some(BusDomMessage { src, dst, payload })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BusSubMessage<'a> {
    src: RefAddr,
    dst: RefAddr,

    #[serde(borrow)]
    payload: BusSubPayload<'a>,
}

impl<'a> BusSubMessage<'a> {
    pub fn reroot(self, arc: &SlabArc<TOTAL_SLABS, SLAB_SIZE>) -> Option<BusSubMessage<'static>> {
        let BusSubMessage { src, dst, payload } = self;


        // See https://github.com/rust-lang/rust/issues/88423 for why we need to
        // be so verbose here.
        let payload: BusSubPayload<'static> = match payload {
            BusSubPayload::Opaque(p) => BusSubPayload::Opaque(p.reroot(arc)?),
            BusSubPayload::DiscoverAck {
                own_id,
                own_id_rand_checksum,
                own_random,
            } => BusSubPayload::DiscoverAck {
                own_id,
                own_id_rand_checksum,
                own_random,
            },
            BusSubPayload::BusGrantAccept {
                tx_bytes_ready,
                rx_bytes_avail,
            } => BusSubPayload::BusGrantAccept {
                tx_bytes_ready,
                rx_bytes_avail,
            },
            BusSubPayload::BusGrantRelease => BusSubPayload::BusGrantRelease,
        };
        Some(BusSubMessage { src, dst, payload })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RefAddr {
    bytes: Vec<u8, MAX_ADDR_SEGMENTS>,
}

impl RefAddr {
    pub fn from_addrs(bytes: &[u8]) -> Result<Self, ()> {
        Vec::from_slice(bytes).map(|v| Self { bytes: v })
    }

    pub fn local_dom_addr() -> Self {
        let mut vec = Vec::new();
        vec.push(0x00).ok();
        Self { bytes: vec }
    }

    pub fn local_broadcast_addr() -> Self {
        let mut vec = Vec::new();
        vec.push(0xFF).ok();
        Self { bytes: vec }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub enum BusDomPayload<'a> {
    ResetConnection,

    #[serde(borrow)]
    Opaque(ManagedArcSlab<'a, TOTAL_SLABS, SLAB_SIZE>),
    DiscoverInitial {
        random: u32,
        min_wait_us: u32,
        max_wait_us: u32,

        #[serde(borrow)]
        offers: ManagedArcSlab<'a, TOTAL_SLABS, SLAB_SIZE>,
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

impl<'a> BusDomPayload<'a> {}

#[derive(Debug, Serialize, Deserialize)]
pub enum BusSubPayload<'a> {
    #[serde(borrow)]
    Opaque(ManagedArcSlab<'a, TOTAL_SLABS, SLAB_SIZE>),
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
