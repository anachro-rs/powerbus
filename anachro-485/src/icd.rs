pub use byte_slab::ManagedArcSlab;
pub use heapless::Vec;
use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::{dispatch::LocalHeader, dom::MANAGEMENT_PORT, HeaderPacket};

pub const MAX_ADDR_SEGMENTS: usize = 8;

// These should prooooobably be configurable
pub const TOTAL_SLABS: usize = 128;
pub const SLAB_SIZE: usize = 512;

// Reserved addrs
pub const LOCAL_DOM_ADDR: u8 = 0;
pub const LOCAL_BROADCAST_ADDR: u8 = 255;

pub const LOCAL_ADDR_LEN: usize = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddrPort {
    pub(crate) addr: VecAddr,
    pub(crate) port: u16,
}

impl AddrPort {
    pub fn from_parts(addr: VecAddr, port: u16) -> Self {
        Self { addr, port }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LineHeader {
    pub(crate) src: AddrPort,
    pub(crate) dst: AddrPort,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LineMessage<'a> {
    pub(crate) hdr: LineHeader,

    #[serde(borrow)]
    pub(crate) msg: ManagedArcSlab<'a, TOTAL_SLABS, SLAB_SIZE>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VecAddr {
    bytes: Vec<u8, MAX_ADDR_SEGMENTS>,
}

impl VecAddr {
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
        vec.push(LOCAL_DOM_ADDR).ok();
        Self { bytes: vec }
    }

    pub fn local_broadcast_addr() -> Self {
        let mut vec = Vec::new();
        vec.push(LOCAL_BROADCAST_ADDR).ok();
        Self { bytes: vec }
    }

    pub fn get_exact_local_addr(&self) -> Option<u8> {
        if self.bytes.len() != LOCAL_ADDR_LEN {
            // Not a local addr, has a chain
            return None;
        }
        self.bytes.get(0).cloned()
    }
}

pub const MAX_OFFERS: usize = 32;

#[derive(Debug, Serialize, Deserialize)]
pub enum BusDomPayload {
    ResetConnection,
    DiscoverInitial {
        random: u32,
        min_wait_us: u32,
        max_wait_us: u32,

        offers: Vec<u8, MAX_OFFERS>,
    },
    DiscoverAckAck {
        own_id: u8,
        own_random: u32,
        own_id_ownrand_checksum: u32,
    },
    PingReq {
        random: u32,
        min_wait_us: u32,
        max_wait_us: u32,
    },
    BusGrant {
        tx_bytes_ready: u32,
        rx_bytes_avail: u32,
        max_grant_us: u32,
    },
}

impl BusDomPayload {
    pub fn generate_discover_ack_ack(
        addr: u8,
        dom_random: u32,
        sub_random: u32,
    ) -> HeaderPacket<BusDomPayload> {
        HeaderPacket {
            hdr: LocalHeader {
                src: AddrPort::from_parts(VecAddr::local_dom_addr(), MANAGEMENT_PORT),
                dst: AddrPort::from_parts(VecAddr::from_local_addr(addr), MANAGEMENT_PORT),
                tick: 0,
            },

            body: BusDomPayload::DiscoverAckAck {
                own_id: addr,
                own_random: dom_random,
                own_id_ownrand_checksum: checksum_addr_random(addr, dom_random, sub_random),
            },
        }
    }

    pub fn validate_discover_ack_ack(&self, hdr: &LocalHeader, sub_random: u32) -> Result<u8, ()> {
        let src_addr = hdr.src.addr.get_exact_local_addr().ok_or(())?;
        let dst_addr = hdr.dst.addr.get_exact_local_addr().ok_or(())?;

        if let BusDomPayload::DiscoverAckAck {
            own_id,
            own_random,
            own_id_ownrand_checksum,
        } = self
        {
            if (dst_addr != *own_id) || (src_addr != 0) {
                return Err(());
            }

            let value = checksum_addr_random(*own_id, *own_random, sub_random);
            if value != *own_id_ownrand_checksum {
                return Err(());
            }

            Ok(*own_id)
        } else {
            return Err(());
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub enum BusSubPayload {
    DiscoverAck {
        own_id: u8,
        own_id_rand_checksum: u32,
        own_random: u32,
    },
    BusGrantAccept {
        tx_bytes_ready: u32,
        rx_bytes_avail: u32,
    },
    PingAck {
        own_id_checksum: u32,
        own_random: u32,
    },
    BusGrantRelease,
}

impl BusSubPayload {
    pub fn validate_ping_ack(&self, hdr: &LocalHeader, dom_random: u32) -> Result<(), ()> {
        // Messages must come from the local bus
        let addr = hdr.src.addr.get_exact_local_addr().ok_or(())?;

        if let BusSubPayload::PingAck {
            own_id_checksum,
            own_random,
        } = self
        {
            // Terrible checksum!
            let result = checksum_addr_random(addr, dom_random, *own_random);

            if *own_id_checksum == result {
                Ok(())
            } else {
                println!("BAD CKSM");
                Err(())
            }
        } else {
            println!("WRONG MSG");
            Err(())
        }
    }

    pub fn validate_discover_ack_addr(
        &self,
        hdr: &LocalHeader,
        dom_random: u32,
    ) -> Result<(u8, u32), ()> {
        // Messages must come from the local bus
        let addr = hdr.src.addr.get_exact_local_addr().ok_or(())?;

        if let BusSubPayload::DiscoverAck {
            own_id,
            own_id_rand_checksum,
            own_random,
        } = self
        {
            // Source address must match claim address
            if *own_id != addr {
                println!("BAD ADDR");
                return Err(());
            }

            // Terrible checksum!
            let result = checksum_addr_random(*own_id, dom_random, *own_random);

            if *own_id_rand_checksum == result {
                Ok((addr, *own_random))
            } else {
                println!("BAD CKSM");
                Err(())
            }
        } else {
            println!("WRONG MSG");
            Err(())
        }
    }

    pub fn generate_discover_ack<R: Rng>(
        rng: &mut R,
        dom: BusDomPayload,
        hdr: &LocalHeader,
    ) -> Option<(u8, u32, u32, HeaderPacket<BusSubPayload>)> {
        let src = hdr.src.addr.get_exact_local_addr()?;
        let dst = hdr.dst.addr.get_exact_local_addr()?;

        if (src != 0) || (dst != 255) {
            return None;
        }

        if let BusDomPayload::DiscoverInitial {
            random,
            min_wait_us,
            max_wait_us,
            offers,
        } = dom
        {
            let delay = rng.gen_range(min_wait_us..max_wait_us);
            let addr_idx = rng.gen_range(0..offers.len());
            let addr = *offers.get(addr_idx)?;
            let sub_random = rng.gen();

            Some((
                addr,
                sub_random,
                delay,
                HeaderPacket {
                    hdr: LocalHeader {
                        src: hdr.dst.clone(),
                        dst: hdr.src.clone(),
                        tick: 0,
                    },
                    body: BusSubPayload::DiscoverAck {
                        own_id: addr,
                        own_random: sub_random,
                        own_id_rand_checksum: checksum_addr_random(addr, random, sub_random),
                    },
                },
            ))
        } else {
            None
        }
    }

    pub fn generate_ping_ack<R: Rng>(
        rng: &mut R,
        own_addr: u8,
        dom: BusDomPayload,
        hdr: &LocalHeader,
    ) -> Option<(u32, HeaderPacket<BusSubPayload>)> {
        let src = hdr.src.addr.get_exact_local_addr()?;
        let dst = hdr.dst.addr.get_exact_local_addr()?;

        if (src != 0) || (dst != own_addr) {
            return None;
        }

        if let BusDomPayload::PingReq {
            random,
            min_wait_us,
            max_wait_us,
        } = dom
        {
            let rand = rng.gen();
            let jitter = rng.gen_range(min_wait_us..max_wait_us);

            let resp = HeaderPacket {
                hdr: LocalHeader {
                    dst: hdr.src.clone(),
                    src: hdr.dst.clone(),
                    tick: 0,
                },
                body: BusSubPayload::PingAck {
                    own_id_checksum: checksum_addr_random(own_addr, random, rand),
                    own_random: rand,
                },
            };

            Some((jitter, resp))
        } else {
            None
        }
    }
}

pub fn checksum_addr_random(addr: u8, dom_random: u32, sub_random: u32) -> u32 {
    let id_word = u32::from_ne_bytes([addr; 4]);
    id_word
        .wrapping_mul(dom_random)
        .wrapping_add(dom_random)
        .wrapping_mul(sub_random)
        .wrapping_add(sub_random)
}
