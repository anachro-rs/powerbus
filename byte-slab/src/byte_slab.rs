//! A slab of byte-array elements
//!
//! The `BSlab` represents the storage of all boxes and their related metadata.
//!
//! Currently, it maintains its free list as an MPMC queue, though that is an implementation
//! detail that may change. This implementation is convenient, but not particularly memory-dense.
//!
//! The slab is statically allocated, and the size of each Box, as well as the total number of
//! Boxes available is selected through compile time `const` values.

use crate::slab_box::SlabBox;
use core::{
    cell::UnsafeCell,
    mem::MaybeUninit,
    slice::from_raw_parts,
    sync::atomic::{AtomicU8, AtomicUsize, Ordering},
};
use heapless::mpmc::MpMcQueue;

/// A slab of byte-array elements
///
/// A BSlab is intended to be allocated as a static item. The const constructor,
/// returns an item containing only zeros, meaning that it will be placed in the
/// `.bss` section, meaning it will not use up flash memory space.
///
/// A BSlab has two generic parameters, both `usize`s:
///
/// * `N`: The number of allocatable elements contained by the Slab
/// * `SZ`: The size (in bytes) of each element
///
/// For example, a `BSlab<8, 128>` would contain eight, 128-byte elements. Therefore
/// it would have a total storage space of 1024 bytes.
pub struct BSlab<const N: usize, const SZ: usize> {
    /// The underlying storage of the BSlab
    bufs: MaybeUninit<[UnsafeCell<[u8; SZ]>; N]>,

    /// The reference counts for each of the buffers
    arcs: [AtomicUsize; N],

    /// The free-list queue used by the BSlab
    ///
    /// This unsafe abomination is because MpMc does not contain all zeroes
    /// at init time, which "infects" this structure, making the `bufs` also
    /// end up in `.data` instead of `.bss` which makes the firmware much
    /// larger, and the flashing much slower. This is a workaround to force
    /// the contents to be in `.bss`.
    alloc_q: UnsafeCell<MaybeUninit<MpMcQueue<usize, N>>>,

    // Initialization state
    state: AtomicU8,
}

// BSlab may be `Sync`, as all safety elements are checked at runtime
// using ato
unsafe impl<const N: usize, const SZ: usize> Sync for BSlab<N, SZ> {}

/// A token representing access to a single slab element. Used for
/// internal unsafe access
pub(crate) struct SlabIdxData<const SZ: usize> {
    pub(crate) buf: &'static UnsafeCell<[u8; SZ]>,
    pub(crate) arc: &'static AtomicUsize,
}

// TODO: I should switch to `atomic-polyfill` to support thumbv6
/// Storage of a slab of runtime-allocatable byte chunks
impl<const N: usize, const SZ: usize> BSlab<N, SZ> {

    /// Create a new `BSlab` in a constant context.
    ///
    /// NOTE: The `BSlab` MUST be initialized with a call to `BSlab::init()` before
    /// usage, or all allocations will fail!
    pub const fn new() -> Self {
        // thanks, mara, for the const repeated initializer trick!
        const ZERO_ARC: AtomicUsize = AtomicUsize::new(0);
        Self {
            bufs: MaybeUninit::uninit(),
            arcs: [ZERO_ARC; N],
            alloc_q: UnsafeCell::new(MaybeUninit::uninit()),
            state: AtomicU8::new(Self::UNINIT),
        }
    }

    const UNINIT: u8 = 0;
    const INITIALIZING: u8 = 1;
    const INITIALIZED: u8 = 2;

    /// Is the buffer initialized?
    pub fn is_init(&self) -> Result<(), ()> {
        if Self::INITIALIZED == self.state.load(Ordering::SeqCst) {
            Ok(())
        } else {
            Err(())
        }
    }

    pub(crate) fn get_q(&self) -> Result<&MpMcQueue<usize, N>, ()> {
        self.is_init()?;
        unsafe {
            Ok(&*(*self.alloc_q.get()).as_ptr())
        }
    }

    /// Allocate a new Box of `SZ`.
    ///
    /// This function will return `None` if the buffer has not been initialized,
    /// or if there are no pages available.
    pub fn alloc_box(&'static self) -> Option<SlabBox<N, SZ>> {
        let idx = self.get_q().ok()?.dequeue()?;
        let arc = unsafe { self.get_idx_unchecked(idx).arc };

        // Store a refcount of one. This box was not previously allocated,
        // so we can disregard the previous value
        arc.store(1, Ordering::SeqCst);

        Some(SlabBox { slab: self, idx })
    }

    /// Get the metadata handle for a given index
    pub(crate) unsafe fn get_idx_unchecked(&'static self, idx: usize) -> SlabIdxData<SZ> {
        SlabIdxData {
            buf: &*self.bufs.as_ptr().cast::<UnsafeCell<[u8; SZ]>>().add(idx),
            arc: &self.arcs[idx],
        }
    }

    /// Initialize the buffer.
    ///
    /// This function will fail if the BSlab has already been initialized, OR if it
    /// is already in-process of being initialized (e.g. in a multithreaded context).
    pub fn init(&self) -> Result<(), ()> {
        // Begin initialization. Returns an error if the slab was not previously
        // uninitialized
        self.state
            .compare_exchange(
                Self::UNINIT,
                Self::INITIALIZING,
                Ordering::SeqCst,
                Ordering::SeqCst,
            )
            .map_err(drop)?;

        // Initialize the alloc_q
        let good_q = unsafe {
            (*self.alloc_q.get()).as_mut_ptr().write(MpMcQueue::new());
            &*(*self.alloc_q.get()).as_ptr()
        };

        // Initialize each slab to zero to prevent UB
        unsafe {
            let buf_ptr = self.bufs.as_ptr().cast::<UnsafeCell<[u8; SZ]>>();
            let bufs_slice: &[UnsafeCell<[u8; SZ]>] = from_raw_parts(buf_ptr, N);
            for slab in bufs_slice {
                // Set each unsafecell to zero
                slab.get().write_bytes(0x00, 1);
            }
        }

        // Add all slabs to the allocation queue
        for i in 0..N {
            good_q.enqueue(i).map_err(drop)?;
        }

        // Complete initialization. Returns an error if the slab was not previously
        // uninitialized
        self.state
            .compare_exchange(
                Self::INITIALIZING,
                Self::INITIALIZED,
                Ordering::SeqCst,
                Ordering::SeqCst,
            )
            .map_err(drop)?;

        Ok(())
    }
}
