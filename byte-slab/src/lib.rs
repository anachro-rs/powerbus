// #![cfg_attr(not(test), no_std)]

/// # Byte Slab
///
/// TODO: This probably does not handle unwind safety correctly!
/// Please verify before using in non-abort-panic environments!

use core::{
    sync::atomic::{AtomicU8, AtomicUsize, Ordering},
    cell::UnsafeCell,
    mem::{forget, MaybeUninit},
    slice::from_raw_parts,
    ops::{Deref, DerefMut},
};
use core::marker::PhantomData;
pub use heapless::mpmc::MpMcQueue;

// TODO: This doesn't HAVE to be 'static, but it makes my life easier
// if you want not-that, I guess open an issue and let me know?
pub struct SlabBox<const N: usize, const SZ: usize> {
    slab: &'static BSlab<N, SZ>,
    idx: usize,
}

// TODO: This doesn't HAVE to be 'static, but it makes my life easier
// if you want not-that, I guess open an issue and let me know?
pub struct SlabArc<const N: usize, const SZ: usize> {
    slab: &'static BSlab<N, SZ>,
    idx: usize,
}

// TODO: This doesn't HAVE to be 'static, but it makes my life easier
// if you want not-that, I guess open an issue and let me know?
#[derive(Clone)]
pub struct SlabSliceArc<const N: usize, const SZ: usize> {
    arc: SlabArc<N, SZ>,
    start: usize,
    len: usize,
}

#[derive(Clone)]
pub enum ManagedArcSlab<'a, const N: usize, const SZ: usize> {
    Borrowed(&'a [u8]),
    Owned(SlabSliceArc<N, SZ>)
}

// ------ SLAB BOX

impl<const N: usize, const SZ: usize> Drop for SlabBox<N, SZ> {
    fn drop(&mut self) {
        let arc = unsafe { self.slab.get_idx_unchecked(self.idx).arc };

        // drop refct
        let zero = arc.compare_exchange(
            1,
            0,
            Ordering::SeqCst,
            Ordering::SeqCst
        );
        // TODO: Make debug assert?
        assert!(zero.is_ok());
        let push = self.slab.alloc_q.enqueue(self.idx);
        assert!(push.is_ok());

        // TODO: Zero on drop? As option?
    }
}

impl<const N: usize, const SZ: usize> Deref for SlabBox<N, SZ> {
    type Target = [u8; SZ];

    fn deref(&self) -> &Self::Target {
        let buf = unsafe { self.slab.get_idx_unchecked(self.idx).buf };

        unsafe {
            &*buf.get()
        }
    }
}

impl<const N: usize, const SZ: usize> DerefMut for SlabBox<N, SZ> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        let buf = unsafe { self.slab.get_idx_unchecked(self.idx).buf };

        unsafe {
            &mut *buf.get()
        }
    }
}

impl<const N: usize, const SZ: usize> SlabBox<N, SZ> {
    pub fn into_arc(self) -> SlabArc<N, SZ> {
        let arc = unsafe { self.slab.get_idx_unchecked(self.idx).arc };

        let refct = arc.load(Ordering::SeqCst);
        assert_eq!(1, refct);

        let new_arc = SlabArc {
            slab: self.slab,
            idx: self.idx,
        };

        // Forget the box to avoid the destructor
        forget(self);

        new_arc
    }

    pub fn slab(&self) -> &'static BSlab<N, SZ> {
        self.slab
    }
}

// ------ SLAB ARC

impl<const N: usize, const SZ: usize> SlabArc<N, SZ> {
    pub fn full_sub_slice_arc(&self) -> SlabSliceArc<N, SZ> {
        SlabSliceArc {
            arc: self.clone(),
            start: 0,
            len: self.len(),
        }
    }

    pub fn sub_slice_arc(&self, start: usize, len: usize) -> Result<SlabSliceArc<N, SZ>, ()> {
        let new_arc = self.clone();

        let new_slice_arc = SlabSliceArc {
            arc: new_arc,
            start,
            len,
        };

        let good_start = start < SZ;
        let good_len = (start + len) <= SZ;

        if good_start && good_len {
            Ok(new_slice_arc)
        } else {
            Err(())
        }
    }

    pub fn slab(&self) -> &'static BSlab<N, SZ> {
        self.slab
    }
}

impl<const N: usize, const SZ: usize> Drop for SlabArc<N, SZ> {
    fn drop(&mut self) {
        // drop refct
        let arc = unsafe { self.slab.get_idx_unchecked(self.idx).arc };
        let refct = arc.fetch_sub(1, Ordering::SeqCst);

        // We just dropped the refct to zero. Release the structure
        if refct == 1 {
            let push = self.slab.alloc_q.enqueue(self.idx);
            assert!(push.is_ok());
        }
    }
}

impl<const N: usize, const SZ: usize> Deref for SlabArc<N, SZ> {
    type Target = [u8; SZ];

    fn deref(&self) -> &Self::Target {
        let buf = unsafe { self.slab.get_idx_unchecked(self.idx).buf };

        unsafe {
            &*buf.get()
        }
    }
}

impl<const N: usize, const SZ: usize> Clone for SlabArc<N, SZ> {
    fn clone(&self) -> Self {
        let arc = unsafe { self.slab.get_idx_unchecked(self.idx).arc };

        let old_ct = arc.fetch_add(1, Ordering::SeqCst);
        assert!(old_ct >= 1);

        Self {
            slab: self.slab,
            idx: self.idx,
        }
    }
}

// ----------- SLAB SLICE ARC

impl<const N: usize, const SZ: usize> Deref for SlabSliceArc<N, SZ> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        // thanks mara for the cleaner slice syntax!
        &self.arc.deref()[self.start..][..self.len]
    }
}

impl<const N: usize, const SZ: usize> SlabSliceArc<N, SZ> {
    pub fn sub_slice_arc(&self, start: usize, len: usize) -> Result<SlabSliceArc<N, SZ>, ()> {
        let new_arc = self.arc.clone();

        // Offset inside of our own slice
        let start = self.start + start;

        let new_slice_arc = SlabSliceArc {
            arc: new_arc,
            start,
            len,
        };

        let new_end = self.start + self.len;
        let good_start = start < new_end;
        let good_len = (start + len) <= new_end;

        if good_start && good_len {
            Ok(new_slice_arc)
        } else {
            Err(())
        }
    }

    pub fn slab(&self) -> &'static BSlab<N, SZ> {
        self.arc.slab()
    }
}

use core::fmt::Debug;
use serde::ser::Serialize;
use serde::de::Deserialize;

impl<'a, const N: usize, const SZ: usize> Debug for ManagedArcSlab<'a, N, SZ> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // TODO: Probably want a better debug impl than this
        match self {
            ManagedArcSlab::Borrowed(b) => b.fmt(f),
            ManagedArcSlab::Owned(o) => o.deref().fmt(f),
        }
    }
}

impl<'a, const N: usize, const SZ: usize> Serialize for ManagedArcSlab<'a, N, SZ> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer
    {
        let data: &[u8] = self.deref();
        data.serialize(serializer)
    }
}


impl<'de: 'a, 'a, const N: usize, const SZ: usize> Deserialize<'de> for ManagedArcSlab<'a, N, SZ> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>
    {
        struct ByteVisitor<'a, const N: usize, const SZ: usize> {
            pd: PhantomData<&'a ()>,
        }

        impl<'d: 'ai, 'ai, const NI: usize, const SZI: usize> serde::de::Visitor<'d> for ByteVisitor<'ai, NI, SZI> {
            type Value = ManagedArcSlab<'ai, NI, SZI>;

            fn expecting(&self, _formatter: &mut core::fmt::Formatter) -> core::fmt::Result {
                todo!()
            }

            fn visit_borrowed_bytes<E>(self, v: &'d [u8]) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(ManagedArcSlab::Borrowed(v))
            }
        }
        deserializer.deserialize_bytes(ByteVisitor { pd: PhantomData })
    }
}

impl<'a, const N: usize, const SZ: usize> Deref for ManagedArcSlab<'a, N, SZ> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        match self {
            ManagedArcSlab::Borrowed(data) => data,
            ManagedArcSlab::Owned(ssa) => ssa.deref()
        }
    }
}

impl<'a, const N: usize, const SZ: usize> ManagedArcSlab<'a, N, SZ> {
    pub fn from_arc(arc: &SlabArc<N, SZ>) -> ManagedArcSlab<'static, N, SZ> {
        ManagedArcSlab::Owned(arc.full_sub_slice_arc())
    }

    pub fn from_slice(sli: &'a [u8]) -> ManagedArcSlab<'a, N, SZ> {
        ManagedArcSlab::Borrowed(sli)
    }

    pub fn from_slab_slice_arc(arc: &SlabSliceArc<N, SZ>) -> ManagedArcSlab<'static, N, SZ> {
        ManagedArcSlab::Owned(arc.clone())
    }

    pub fn reroot(self, arc: &SlabArc<N, SZ>) -> Option<ManagedArcSlab<'static, N, SZ>> {
        match self {
            ManagedArcSlab::Owned(e) => Some(ManagedArcSlab::Owned(e)),
            ManagedArcSlab::Borrowed(b) => {
                if arc.is_empty() || b.is_empty() {
                    // TODO: nuance
                    return None;
                }

                // TODO: yolo ub
                let start: usize = arc.deref().as_ptr() as usize;
                let end: usize = start + arc.deref().len();
                let b_start: usize = b.as_ptr() as usize;

                if (start <= b_start) && (b_start < end) {
                    let ssa = arc.sub_slice_arc(b_start - start, b.len()).ok()?;
                    Some(ManagedArcSlab::Owned(ssa))
                } else {
                    None
                }
            }
        }
    }
}


// -----------


// SAFETY: YOLO
unsafe impl<const N: usize, const SZ: usize> Sync for SlabBox<N, SZ> { }
unsafe impl<const N: usize, const SZ: usize> Send for SlabBox<N, SZ> { }

pub struct BSlab<const N: usize, const SZ: usize> {
    bufs: MaybeUninit<[UnsafeCell<[u8; SZ]>; N]>,
    arcs: [AtomicUsize; N],
    alloc_q: MpMcQueue<usize, N>,
    state: AtomicU8,
}

// SAFETY: YOLO
unsafe impl<const N: usize, const SZ: usize> Sync for BSlab<N, SZ> { }

struct SlabIdxData<const SZ: usize> {
    buf: &'static UnsafeCell<[u8; SZ]>,
    arc: &'static AtomicUsize,
}

// TODO: I should switch to `atomic-polyfill` to support thumbv6
impl<const N: usize, const SZ: usize> BSlab<N, SZ> {
    pub const fn new() -> Self {
        // thanks, mara, for the const repeated initializer trick!
        const ZERO_ARC: AtomicUsize = AtomicUsize::new(0);
        Self {
            bufs: MaybeUninit::uninit(),
            arcs: [ZERO_ARC; N],
            alloc_q: MpMcQueue::new(),
            state: AtomicU8::new(0),
        }
    }

    const UNINIT: u8 = 0;
    const INITIALIZING: u8 = 1;
    const INITIALIZED: u8 = 2;

    pub fn is_init(&self) -> Result<(), ()> {
        if Self::INITIALIZED == self.state.load(Ordering::SeqCst) {
            Ok(())
        } else {
            Err(())
        }
    }

    pub fn get_slabs(&self) -> Result<&[UnsafeCell<[u8; SZ]>], ()> {
        self.is_init()?;
        unsafe {
            let buf_ptr = self.bufs.as_ptr().cast::<UnsafeCell<[u8; SZ]>>();
            let bufs_slice: &[UnsafeCell<[u8; SZ]>] = from_raw_parts(buf_ptr, N);
            Ok(bufs_slice)
        }
    }

    pub fn alloc_box(&'static self) -> Option<SlabBox<N,SZ>> {
        self.is_init().ok()?;
        let idx = self.alloc_q.dequeue()?;
        let arc = unsafe { self.get_idx_unchecked(idx).arc };

        // Store a refcount of one. This box was not previously allocated,
        // so we can disregard the previous value
        arc.store(1, Ordering::SeqCst);

        Some(SlabBox {
            slab: self,
            idx,
        })
    }

    unsafe fn get_idx_unchecked(&'static self, idx: usize) -> SlabIdxData<SZ> {
        SlabIdxData {
            buf: &*self.bufs.as_ptr().cast::<UnsafeCell<[u8; SZ]>>().add(idx),
            arc: &self.arcs[idx],
        }
    }

    pub fn init(&self) -> Result<(), ()> {
        // Begin initialization. Returns an error if the slab was not previously
        // uninitialized
        self.state.compare_exchange(
            Self::UNINIT,
            Self::INITIALIZING,
            Ordering::SeqCst,
            Ordering::SeqCst
        ).map_err(drop)?;

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
            self.alloc_q.enqueue(i).map_err(drop)?;
        }

        // Complete initialization. Returns an error if the slab was not previously
        // uninitialized
        self.state.compare_exchange(
            Self::INITIALIZING,
            Self::INITIALIZED,
            Ordering::SeqCst,
            Ordering::SeqCst
        ).map_err(drop)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        static SLAB: BSlab<32, 512> = BSlab::new();
        SLAB.init().unwrap();
        let mut allocs = vec![];

        for i in 0..32 {
            let mut alloc_box = SLAB.alloc_box().unwrap();
            alloc_box[0] = i;
            alloc_box[1] = 0x42;
            allocs.push(alloc_box);
        }

        assert!(SLAB.alloc_box().is_none());

        for (i, ab) in allocs.iter().enumerate() {
            assert_eq!(i as u8, ab[0]);
            assert_eq!(0x42, ab[1]);
        }

        // Drop allocs, freeing them for reuse
        drop(allocs);

        let mut allocs = vec![];

        for _ in 0..32 {
            allocs.push(SLAB.alloc_box().unwrap());
        }

        assert!(SLAB.alloc_box().is_none());
    }

    #[test]
    fn slicing_and_arcs() {
        static SLAB: BSlab<32, 128> = BSlab::new();
        SLAB.init().unwrap();

        let mut slab_box = SLAB.alloc_box().unwrap();
        slab_box.iter_mut().enumerate().for_each(|(i, by)| {
            *by = i as u8;
        });

        let slab_arc = slab_box.into_arc();
        slab_arc.iter().enumerate().for_each(|(i, by)| {
            assert_eq!(*by, i as u8);
        });

        let sl_1 = slab_arc.sub_slice_arc(0, 64).unwrap();
        let sl_2 = slab_arc.sub_slice_arc(64, 64).unwrap();

        sl_1.iter().enumerate().for_each(|(i, by)| {
            assert_eq!(*by, i as u8);
        });
        sl_2.iter().enumerate().for_each(|(i, by)| {
            assert_eq!(*by, i as u8 + 64);
        });

        let sl_2_1 = sl_2.sub_slice_arc(0, 32).unwrap();
        let sl_2_2 = sl_2.sub_slice_arc(32, 32).unwrap();

        sl_2_1.iter().enumerate().for_each(|(i, by)| {
            assert_eq!(*by, i as u8 + 64);
        });
        sl_2_2.iter().enumerate().for_each(|(i, by)| {
            assert_eq!(*by, i as u8 + 64 + 32);
        });

        // We should now be able to allocate EXACTLY 31 pages, not 32.
        let mut allocs = vec![];

        for i in 0..31 {
            let mut alloc_box = SLAB.alloc_box().unwrap();
            alloc_box[0] = i;
            alloc_box[1] = 0x42;
            allocs.push(alloc_box);
        }

        assert!(SLAB.alloc_box().is_none());

        // Now, if we drop the root arc, we still shouldn't be able to alloc.
        drop(slab_arc);
        assert!(SLAB.alloc_box().is_none());

        // Now, the top level slices, still no alloc
        drop(sl_1);
        drop(sl_2);
        assert!(SLAB.alloc_box().is_none());

        // Second to last, still no alloc
        drop(sl_2_1);
        assert!(SLAB.alloc_box().is_none());

        // Final slice. Should be free now.
        drop(sl_2_2);
        assert!(SLAB.alloc_box().is_some());
    }
}
