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
    pub src: RefAddr,
    pub dst: RefAddr,

    #[serde(borrow)]
    pub payload: BusDomPayload<'a>,
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
    pub src: RefAddr,
    pub dst: RefAddr,

    #[serde(borrow)]
    pub payload: BusSubPayload<'a>,
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

    pub fn from_local_addr(addr: u8) -> Self {
        let mut vec = Vec::new();
        vec.push(addr).ok();
        Self { bytes: vec }
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

    pub fn get_exact_local_addr(&self) -> Option<u8> {
        if self.bytes.len() != 1 {
            // Not a local addr, has a chain
            return None;
        }
        self.bytes.get(0).cloned()
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

impl<'a> BusDomMessage<'a> {
    pub fn generate_discover_ack_ack(addr: u8, dom_random: u32, sub_random: u32) -> BusDomMessage<'static> {
        BusDomMessage{
            src: RefAddr::local_dom_addr(),
            dst: RefAddr::from_local_addr(addr),
            payload: BusDomPayload::DiscoverAckAck {
                own_id: addr,
                own_random: dom_random,
                own_id_ownrand_checksum: checksum_addr_random(addr, dom_random, sub_random),
            },
        }
    }
}

impl<'a> BusSubMessage<'a> {
    pub fn validate_discover_ack_addr(&self, dom_random: u32) -> Result<(u8, u32), ()> {
        // Messages must come from the local bus
        let addr = self.src.get_exact_local_addr().ok_or(())?;

        if let BusSubPayload::DiscoverAck { own_id, own_id_rand_checksum, own_random } = self.payload {
            // Source address must match claim address
            if own_id != addr {
                return Err(());
            }

            // Terrible checksum!
            let result = checksum_addr_random(own_id, own_random, dom_random);

            if own_id_rand_checksum == result {
                Ok((addr, own_random))
            } else {
                Err(())
            }
        } else {
            Err(())
        }
    }
}

fn checksum_addr_random(addr: u8, dom_random: u32, sub_random: u32) -> u32 {
    let id_word = u32::from_ne_bytes([addr; 4]);
    id_word
        .wrapping_mul(dom_random)
        .wrapping_add(dom_random)
        .wrapping_mul(sub_random)
        .wrapping_add(sub_random)
}
