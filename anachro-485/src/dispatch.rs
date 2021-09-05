use crate::icd::{
    AddrPort, LineHeader, LineMessage, VecAddr, LOCAL_BROADCAST_ADDR, LOCAL_DOM_ADDR, SLAB_SIZE,
    TOTAL_SLABS,
};
use byte_slab::{BSlab, ManagedArcSlab, SlabBox};
use cobs::decode_in_place;
use serde::Serialize;
use core::{
    num::NonZeroU16,
    ops::DerefMut,
    sync::atomic::{AtomicU16, Ordering::SeqCst},
};
use heapless::mpmc::MpMcQueue;
use postcard::{from_bytes, to_slice, to_slice_cobs};
use std::{ops::Deref, sync::atomic::AtomicU8};

const TASK_QUEUE_DEPTH: usize = 4;
const IO_QUEUE_DEPTH: usize = 32;

type BBox = SlabBox<TOTAL_SLABS, SLAB_SIZE>;
type AllocSlab = BSlab<TOTAL_SLABS, SLAB_SIZE>;
type MASlab = ManagedArcSlab<'static, TOTAL_SLABS, SLAB_SIZE>;

pub struct TimeStampBox {
    packet: BBox,
    tick: u32,
}

#[derive(Debug)]
pub struct LocalHeader {
    pub src: AddrPort,
    pub dst: AddrPort,
    pub tick: u32,
}

pub struct LocalPacket {
    pub(crate) hdr: LocalHeader,
    pub(crate) payload: MASlab,
}

impl LocalPacket {
    pub fn from_parts_with_alloc<T: Serialize>(
        msg: T,
        src: AddrPort,
        dst: AddrPort,
        allo: &'static AllocSlab,
    ) -> Option<Self> {
        let mut buf = allo.alloc_box()?;
        let len = to_slice(&msg, buf.deref_mut()).ok()?.len();
        let arc = buf.into_arc();
        let ssa = arc.sub_slice_arc(0, len).ok()?;

        let lcp = LocalPacket {
            hdr: LocalHeader {
                src,
                dst,

                // TODO: record tick?
                tick: 0,
            },
            payload: ManagedArcSlab::Owned(ssa),
        };

        Some(lcp)
    }
}

struct PortQueue {
    port: AtomicU16,
    to_task: MpMcQueue<LocalPacket, TASK_QUEUE_DEPTH>,
    to_dispatch: MpMcQueue<LocalPacket, TASK_QUEUE_DEPTH>,
}

pub struct IoQueue {
    to_io: MpMcQueue<MASlab, IO_QUEUE_DEPTH>,
    to_dispatch: MpMcQueue<TimeStampBox, IO_QUEUE_DEPTH>,
}

impl IoQueue {
    pub const fn new() -> Self {
        Self {
            to_io: MpMcQueue::new(),
            to_dispatch: MpMcQueue::new(),
        }
    }
}

/// Message dispatch and routing
///
/// NOTE: This struct intentionally has NO way to de-allocate ports
/// that have been assigned. It has not (yet) been designed with
/// the ability to deprovision correctly, and is intended for all ports
/// to be assigned once, from a single thread, at the top of the
/// program. All other uses beware (for now)
pub struct Dispatch<const PORTS: usize> {
    ports: [PortQueue; PORTS],
    ioq: &'static IoQueue,
    own_addr: AtomicU8,
    shame: MpMcQueue<MASlab, 1>,
    alloc: &'static AllocSlab,
    // TODO: link to another Dispatch for forwarding
}

pub const INVALID_PORT: u16 = 0;
pub const INVALID_OWN_ADDR: u8 = LOCAL_BROADCAST_ADDR;

pub enum ProcessMessageError {
    Cobs,
    Deser,
    ReRoot,
    Arc,
    SrcAddr,
    DestAddr,
    DestPort,
    TaskQueueFull,
    IoQueueFull,
    NoAlloc,
    Ser,
}

impl<const PORTS: usize> Dispatch<PORTS> {
    pub const fn new(ioq: &'static IoQueue, alloc: &'static AllocSlab) -> Self {
        const SINGLE_ITEM: PortQueue = PortQueue {
            port: AtomicU16::new(INVALID_PORT),
            to_task: MpMcQueue::new(),
            to_dispatch: MpMcQueue::new(),
        };

        Self {
            ports: [SINGLE_ITEM; PORTS],
            ioq,
            own_addr: AtomicU8::new(INVALID_OWN_ADDR),
            shame: MpMcQueue::new(),
            alloc,
        }
    }

    /// Register a port, and receive a socket for the corresponding port.
    /// It will return None if:
    ///
    /// * The requested port is zero (not allowed)
    /// * We have already allocated the maximum number of port (e.g. `PORTS`)
    /// * The request port has already been allocated
    pub fn register_port<'a>(&'a self, port: u16) -> Option<DispatchSocket<'a>> {
        // Is the user requesting a valid (non-zero) port?
        let nzport = NonZeroU16::new(port)?;

        // Has this port already been allocated?
        //
        // TODO: This could be racy with the next section! For now,
        // I only plan to do this in a single threaded fashion, but this
        // COULD allow for two tasks to define the same port, in which
        // case the latter port would always be starved. This isn't
        // unsafe, but is undesirable
        //
        // This could be prevented with a "doing management" mutex/spinlock,
        // for now: buyer beware
        if self.ports.iter().any(|p| p.port.load(SeqCst) == port) {
            return None;
        }

        // Try to find a free port.
        self.ports
            .iter()
            .find(|p| {
                // Find/Allocate the slot
                p.port.compare_exchange(INVALID_PORT, port, SeqCst, SeqCst).is_ok()
            })
            .map(|slot| {
                // Return an allocated slot
                DispatchSocket {
                    port: nzport,
                    to_task: &slot.to_task,
                    to_dispatch: &slot.to_dispatch,
                }
            })
    }

    fn process_one_incoming(&self, mut tsb: TimeStampBox) -> Result<(), ProcessMessageError> {
        // de-cobs
        let time = tsb.tick;
        let own_addr = self.own_addr.load(SeqCst);

        let len = decode_in_place(tsb.packet.deref_mut()).map_err(|_| ProcessMessageError::Cobs)?;

        let arc = tsb.packet.into_arc();
        let msg = arc
            .sub_slice_arc(0, len)
            .map_err(|_| ProcessMessageError::Arc)?;

        // deserialize to LineMessage
        let lm = from_bytes::<LineMessage>(msg.deref()).map_err(|_| ProcessMessageError::Deser)?;

        // Check address
        // TODO: Routing?
        match lm.hdr.dst.addr.get_exact_local_addr() {
            // Accept broadcast messages
            // NOTE: This is important before we are assigned an address!
            // (and after, because we use broadcast as the 'invalid' own
            // addr)
            Some(LOCAL_BROADCAST_ADDR) => {}

            // Accept messages to us
            Some(addr) if addr == own_addr => {}

            // Reject all others
            _ => return Err(ProcessMessageError::DestAddr),
        }

        let good = lm
            .hdr
            .src
            .addr
            .get_exact_local_addr()
            .map(|addr| {
                if own_addr == LOCAL_DOM_ADDR {
                    // If we are a DOM, don't accept broadcast or DOM as the source
                    // TODO: actually check allocation of addresses?
                    !(addr == LOCAL_BROADCAST_ADDR || addr == LOCAL_DOM_ADDR)
                } else {
                    // If we are sub, the message must come from the dom
                    addr == LOCAL_DOM_ADDR
                }
            })
            .unwrap_or(false);

        if !good {
            return Err(ProcessMessageError::SrcAddr);
        }

        // Check if we have a matching destination port
        let pq = self
            .ports
            .iter()
            .find(|pq| pq.port.load(SeqCst) == lm.hdr.dst.port)
            .ok_or(ProcessMessageError::DestPort)?;

        // Ship it!
        pq.to_task
            .enqueue(LocalPacket {
                hdr: LocalHeader {
                    src: lm.hdr.src,
                    dst: lm.hdr.dst,
                    tick: time,
                },
                payload: lm.msg.reroot(&arc).ok_or(ProcessMessageError::ReRoot)?,
            })
            .map_err(|_| ProcessMessageError::TaskQueueFull)
    }

    pub fn process_messages(&self) {
        while let Some(msg) = self.ioq.to_dispatch.dequeue() {
            if let Err(_e) = self.process_one_incoming(msg) {
                // TODO: print errors, but dont return early.
            }
        }

        // We can't send as the broadcast addr, don't bother
        // processing outgoing packets yet
        if self.own_addr.load(SeqCst) == LOCAL_BROADCAST_ADDR {
            return;
        }

        // Did we leave a packet stranded?
        if let Some(msg) = self.shame.dequeue() {
            if let Err(msg) = self.ioq.to_io.enqueue(msg) {
                self.shame.enqueue(msg).ok();
                return;
            }
        }

        // TODO: not really fair, gives prio to lower port numbers
        'port: for pq in self.ports.iter() {
            loop {
                // check if there is an allocation available FIRST, to avoid
                // having a packet but no alloc
                let boxy = if let Some(boxy) = self.alloc.alloc_box() {
                    boxy
                } else {
                    return;
                };

                if let Some(msg) = pq.to_dispatch.dequeue() {
                    if let Err(_e) = self.process_one_outgoing(msg, pq.port.load(SeqCst), boxy) {
                        return;
                    }
                } else {
                    continue 'port;
                }
            }
        }
    }

    fn process_one_outgoing(
        &self,
        mut lp: LocalPacket,
        port: u16,
        mut boxy: BBox,
    ) -> Result<(), ProcessMessageError> {
        let own_addr = self.own_addr.load(SeqCst);

        // We shouldn't lie about our own address
        lp.hdr.src.addr = VecAddr::from_local_addr(own_addr);
        lp.hdr.src.port = port;

        let ogp = LineMessage {
            hdr: LineHeader {
                src: lp.hdr.src,
                dst: lp.hdr.dst,
            },
            msg: lp.payload,
        };

        let len = to_slice_cobs(&ogp, boxy.deref_mut())
            .map_err(|_| ProcessMessageError::Ser)?
            .len();

        let arc = boxy.into_arc();
        let ssa = arc
            .sub_slice_arc(0, len)
            .map_err(|_| ProcessMessageError::Arc)?;

        self.ioq
            .to_io
            .enqueue(ManagedArcSlab::Owned(ssa))
            .map_err(|ssa| {
                self.shame.enqueue(ssa).ok();
                ProcessMessageError::IoQueueFull
            })
    }
}

pub struct DispatchSocket<'a> {
    port: NonZeroU16,
    to_task: &'a MpMcQueue<LocalPacket, TASK_QUEUE_DEPTH>,
    to_dispatch: &'a MpMcQueue<LocalPacket, TASK_QUEUE_DEPTH>,
}

impl<'a> DispatchSocket<'a> {
    pub fn try_send(&self, pkt: LocalPacket) -> Result<(), LocalPacket> {
        self.to_dispatch.enqueue(pkt)
    }

    pub fn try_recv(&self) -> Option<LocalPacket> {
        self.to_task.dequeue()
    }

    pub fn port(&self) -> NonZeroU16 {
        self.port
    }
}
