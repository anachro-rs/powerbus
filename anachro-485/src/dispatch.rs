use crate::icd::{
    AddrPort, LineHeader, LineMessage, VecAddr, LOCAL_BROADCAST_ADDR, LOCAL_DOM_ADDR, SLAB_SIZE,
    TOTAL_SLABS,
};

use core::{
    num::NonZeroU16,
    ops::{Deref, DerefMut},
    sync::atomic::{AtomicBool, AtomicU16, AtomicU8, Ordering::SeqCst},
};

use byte_slab::{BSlab, ManagedArcSlab, SlabBox, Reroot};
use cobs::decode_in_place;
use heapless::mpmc::MpMcQueue;
use postcard::{from_bytes, to_slice, to_slice_cobs};
use serde::Serialize;

const TASK_QUEUE_DEPTH: usize = 4;
const IO_QUEUE_DEPTH: usize = 32;

type BBox = SlabBox<TOTAL_SLABS, SLAB_SIZE>;
type AllocSlab = BSlab<TOTAL_SLABS, SLAB_SIZE>;
type MASlab = ManagedArcSlab<'static, TOTAL_SLABS, SLAB_SIZE>;

pub struct TimeStampBox {
    pub packet: BBox,
    pub len: usize,
    pub tick: u32,
}

pub struct OutgoingSlab {
    pub packet: MASlab,
    pub receive_ticks_min: Option<u32>,
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
    pub(crate) response_wait_ticks: Option<u32>,
}

pub enum AwakeIoHandler {
    No,
    Yes,
}

impl LocalPacket {
    pub fn from_hdr_payload(hdr: LocalHeader, payload: MASlab) -> Self {
        Self {
            hdr,
            payload,
            response_wait_ticks: None,
        }
    }

    pub fn header(&self) -> &LocalHeader {
        &self.hdr
    }

    pub fn payload(&self) -> &[u8] {
        self.payload.deref()
    }

    pub fn payload_slab(&self) -> &MASlab {
        &self.payload
    }

    pub fn from_parts_with_alloc<T: Serialize>(
        msg: T,
        src: AddrPort,
        dst: AddrPort,
        rx_ticks: Option<u32>,
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
            response_wait_ticks: rx_ticks,
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
    /// A queue of serialized messages sent to the IO handler
    to_io: MpMcQueue<OutgoingSlab, IO_QUEUE_DEPTH>,

    /// A queue of serialized messages sent to the IO handler
    to_io_hi_prio: MpMcQueue<OutgoingSlab, IO_QUEUE_DEPTH>,

    /// A queue of incoming, serialized messages sent to the
    /// dispatch handler
    to_dispatch: MpMcQueue<TimeStampBox, IO_QUEUE_DEPTH>,

    /// Has the IO Handle been given out already?
    io_given: AtomicBool,

    io_auth: IoAuth,
}

/// The control and queue handle, intended to be driven by the IO Handler
pub struct IoHandle {
    ioq: &'static IoQueue,
}

pub struct IoAuth {
    /// Is the IO handler authorized to send a message at will?
    ///
    /// This flag is cleared after sending a single message.
    ///
    /// TODO: The dom basically always is authorized, while the
    /// sub is the one that needs to wait to be authorized. How
    /// to handle this?
    io_send_auth: AtomicBool,

    io_flush_auth: AtomicBool,

    io_empty_auth: AtomicBool,
}

impl IoHandle {
    pub fn push_incoming(&mut self, tsb: TimeStampBox) -> Result<(), TimeStampBox> {
        self.ioq.to_dispatch.enqueue(tsb)
    }

    pub fn pop_outgoing(&mut self) -> Option<OutgoingSlab> {
        match self.ioq.to_io_hi_prio.dequeue() {
            a @ Some(_) => a,
            None => self.ioq.to_io.dequeue(),
        }
    }

    pub fn auth(&self) -> &IoAuth {
        &self.ioq.io_auth
    }
}

impl IoAuth {
    pub fn enable_one_send(&self) {
        self.io_send_auth.store(true, SeqCst);
    }

    pub fn is_send_authd(&self) -> bool {
        self.io_send_auth.load(SeqCst)
    }

    pub fn clear_send_auth(&self) {
        self.io_send_auth.store(false, SeqCst);
    }

    pub fn is_flush_authd(&self) -> bool {
        self.io_flush_auth.swap(false, SeqCst)
    }

    pub fn mark_empty(&self) {
        self.io_empty_auth.store(true, SeqCst);
    }
}

impl IoQueue {
    pub const fn new() -> Self {
        Self {
            to_io: MpMcQueue::new(),
            to_io_hi_prio: MpMcQueue::new(),
            to_dispatch: MpMcQueue::new(),
            io_given: AtomicBool::new(false),
            io_auth: IoAuth {
                io_send_auth: AtomicBool::new(false),
                io_flush_auth: AtomicBool::new(false),
                io_empty_auth: AtomicBool::new(false),
            },
        }
    }

    // TODO: I need to probably have one for each half, the IoHandle
    // (that goes to the hardware I/O), and for Dispatch (which for now
    // just borrows the IoQ itself).
    pub fn take_io_handle(&'static self) -> Option<IoHandle> {
        self.io_given
            .compare_exchange(false, true, SeqCst, SeqCst)
            .ok()?;

        Some(IoHandle { ioq: &self })
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
    shame: MpMcQueue<OutgoingSlab, 2>,
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

    pub fn set_addr(&self, addr: u8) {
        self.own_addr.store(addr, SeqCst);
    }

    pub fn get_addr(&self) -> Option<u8> {
        let addr = self.own_addr.load(SeqCst);
        if addr == INVALID_OWN_ADDR {
            None
        } else {
            Some(addr)
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

        // Should this port have the ability to authorize outgoing messages?
        //
        // Generally limited to management messages and discovery messsages
        let auth = match port {
            crate::dom::DISCOVERY_PORT => Some(&self.ioq.io_auth),
            crate::dom::TOKEN_PORT => Some(&self.ioq.io_auth),
            _ => None,
        };

        // Try to find a free port.
        self.ports
            .iter()
            .find(|p| {
                // Find/Allocate the slot
                p.port
                    .compare_exchange(INVALID_PORT, port, SeqCst, SeqCst)
                    .is_ok()
            })
            .map(|slot| {
                // Return an allocated slot
                DispatchSocket {
                    port: nzport,
                    to_task: &slot.to_task,
                    to_dispatch: &slot.to_dispatch,
                    send_auth: auth,
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
            Some(LOCAL_BROADCAST_ADDR) => Ok(()),

            // Accept messages to us
            Some(addr) if addr == own_addr => Ok(()),

            // Don't alert on dom messages (if they aren't for us)
            Some(LOCAL_DOM_ADDR) => Err(ProcessMessageError::DestAddr),

            // Reject all others
            Some(addr) => {
                defmt::warn!("not for us! {=u8}", addr);
                Err(ProcessMessageError::DestAddr)
            }

            None => {
                defmt::warn!("not for anyone!");
                Err(ProcessMessageError::DestAddr)
            }
        }?;

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

        let rrkey = arc.rerooter_key();

        // Ship it!
        pq.to_task
            .enqueue(LocalPacket {
                hdr: LocalHeader {
                    src: lm.hdr.src,
                    dst: lm.hdr.dst,
                    tick: time,
                },
                payload: lm.msg.reroot(&rrkey).map_err(|_| ProcessMessageError::ReRoot)?,
                response_wait_ticks: None,
            })
            .map_err(|_| ProcessMessageError::TaskQueueFull)
    }

    pub fn process_messages(&self) {
        while let Some(msg) = self.ioq.to_dispatch.dequeue() {
            if let Err(_e) = self.process_one_incoming(msg) {
                // TODO: print errors, but dont return early.
                defmt::error!("message yeeted");
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
        // TODO: Is this a feature?
        //
        // TODO: Hmm, I think this may end up being a problem, or
        // something to deal with. When we need to respond to a SPECIFIC
        // message, like a bus management message, we may instead need to
        // reply with a SPECIFIC response. However, if we've already filled
        // the queue with lower priority messages, there's not much
        // we can do to bypass, other than (hackily) draining the queue
        // first.
        //
        // I wonder how I could handle this, either having MULTIPLE
        // queues (ehhh?) and change the auth flag to auth a specific
        // port? or a priority queue?
        //
        // this is a *little* less problematic for now, where discovery
        // is divergent from actual behavor, but eventually we will have
        // a dom that wants to do other stuff, and even just doing
        // periodic discovery may cause problems, with the totally
        // blocking nature of sending.
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

        let mas = ManagedArcSlab::Owned(ssa);
        let ogs = OutgoingSlab {
            packet: mas,
            receive_ticks_min: lp.response_wait_ticks,
        };

        if (port == crate::dom::DISCOVERY_PORT) || (port == crate::dom::TOKEN_PORT) {
            self.ioq.to_io_hi_prio.enqueue(ogs).ok();
            Ok(())
        } else {
            self.ioq.to_io.enqueue(ogs).map_err(|ssa| {
                self.shame.enqueue(ssa).ok();
                ProcessMessageError::IoQueueFull
            })
        }
    }
}

pub struct DispatchSocket<'a> {
    port: NonZeroU16,
    to_task: &'a MpMcQueue<LocalPacket, TASK_QUEUE_DEPTH>,
    to_dispatch: &'a MpMcQueue<LocalPacket, TASK_QUEUE_DEPTH>,
    send_auth: Option<&'a IoAuth>,
}

impl<'a> DispatchSocket<'a> {
    pub fn try_send(&self, pkt: LocalPacket) -> Result<(), LocalPacket> {
        self.to_dispatch.enqueue(pkt)
    }

    pub fn try_send_authd(&self, pkt: LocalPacket) -> Result<(), LocalPacket> {
        match self.send_auth {
            Some(auth) => {
                self.try_send(pkt)?;
                auth.enable_one_send();
                Ok(())
            }
            None => Err(pkt),
        }
    }

    pub fn try_recv(&self) -> Option<LocalPacket> {
        self.to_task.dequeue()
    }

    pub fn auth_flush(&self) -> Result<(), ()> {
        self.send_auth
            .map(|auth| auth.io_flush_auth.store(true, SeqCst))
            .ok_or(())
    }

    pub fn auth_send(&self) -> Result<(), ()> {
        self.send_auth.map(|auth| auth.enable_one_send()).ok_or(())
    }

    pub fn clear_empty(&self) -> Result<(), ()> {
        self.send_auth
            .map(|auth| auth.io_empty_auth.store(false, SeqCst))
            .ok_or(())
    }

    pub fn is_empty(&self) -> Result<bool, ()> {
        self.send_auth
            .map(|auth| auth.io_empty_auth.swap(false, SeqCst))
            .ok_or(())
    }

    pub fn port(&self) -> NonZeroU16 {
        self.port
    }
}
