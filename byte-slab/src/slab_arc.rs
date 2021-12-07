//! A reference counted allocation
//!
//! A `SlabArc` is obtained by consuming a `SlabBox`. As the underlying allocation
//! may be shared, a `SlabArc` does not allow for the contents to be modified.
//! `SlabArc`s may be cheaply cloned (which increases the reference count), allowing
//! for multiple (immutable) access to the same data. The underlying memory is freed
//! for reuse automatically when the reference count reaches zero.

use crate::slab_slice_arc::SlabSliceArc;
use core::{
    ops::Deref,
    sync::atomic::Ordering,
};

use crate::byte_slab::BSlab;

// TODO: This doesn't HAVE to be 'static, but it makes my life easier
// if you want not-that, I guess open an issue and let me know?
/// A reference counted, BSlab allocated chunk of bytes.
///
/// `SlabArc`s implement the `Deref` trait for access to the underlying allocation
pub struct SlabArc<const N: usize, const SZ: usize> {
    pub(crate) slab: &'static BSlab<N, SZ>,
    pub(crate) idx: usize,
}

impl<const N: usize, const SZ: usize> SlabArc<N, SZ> {
    /// Create a `SlabSliceArc` from this `SlabArc`, with a full view
    /// of the underlying data
    pub fn full_sub_slice_arc(&self) -> SlabSliceArc<N, SZ> {
        SlabSliceArc {
            arc: self.clone(),
            start: 0,
            len: self.len(),
        }
    }

    /// Create a `SlabSliceArc` from this `SlabArc`, with a partial view
    /// of the underlying data.
    ///
    /// This function will fail if `start` and `len` do not describe a valid
    /// region of the `SlabArc`.
    pub fn sub_slice_arc(&self, start: usize, len: usize) -> Result<SlabSliceArc<N, SZ>, ()> {
        let new_arc = self.clone();

        let good_start = start < SZ;
        let good_len = (start + len) <= SZ;

        if good_start && good_len {
            let new_slice_arc = SlabSliceArc {
                arc: new_arc,
                start,
                len,
            };
            Ok(new_slice_arc)
        } else {
            Err(())
        }
    }
}

impl<const N: usize, const SZ: usize> Drop for SlabArc<N, SZ> {
    fn drop(&mut self) {
        // drop refct
        let arc = unsafe { self.slab.get_idx_unchecked(self.idx).arc };
        let refct = arc.fetch_sub(1, Ordering::SeqCst);

        // We just dropped the refct to zero. Release the structure
        if refct == 1 {
            if let Ok(q) = self.slab.get_q() {
                while let Err(_) = q.enqueue(self.idx) {}
            }

        }
    }
}

impl<const N: usize, const SZ: usize> Deref for SlabArc<N, SZ> {
    type Target = [u8; SZ];

    fn deref(&self) -> &Self::Target {
        let buf = unsafe { self.slab.get_idx_unchecked(self.idx).buf };

        unsafe { &*buf.get() }
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
