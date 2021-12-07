//! An owned allocation from a `BSlab`
//!
//! A `SlabBox` may be read or written to (exclusively) by the owner.
//! A `SlabBox` may also be converted into a `SlabArc` in order to be shared.
//! The underlying memory is freed for reuse automatically when the Box has
//! been dropped.

use core::ops::DerefMut;
use core::ops::Deref;
use core::{
    mem::forget,
    sync::atomic::Ordering,
};

use crate::byte_slab::BSlab;
use crate::slab_arc::SlabArc;

// TODO: This doesn't HAVE to be 'static, but it makes my life easier
// if you want not-that, I guess open an issue and let me know?
/// An owned, BSlab allocated chunk of bytes.
///
/// `SlabBox`s implement the `Deref` and `DerefMut` traits for access to
/// the underlying allocation
pub struct SlabBox<const N: usize, const SZ: usize> {
    pub(crate) slab: &'static BSlab<N, SZ>,
    pub(crate) idx: usize,
}

impl<const N: usize, const SZ: usize> Drop for SlabBox<N, SZ> {
    fn drop(&mut self) {
        let arc = unsafe { self.slab.get_idx_unchecked(self.idx).arc };

        // drop refct
        let zero = arc.compare_exchange(1, 0, Ordering::SeqCst, Ordering::SeqCst);
        // TODO: Make debug assert?
        assert!(zero.is_ok());

        // TODO: Why is this necessary?
        if let Ok(q) = self.slab.get_q() {
            while let Err(_) = q.enqueue(self.idx) {}
        }

        // TODO: Zero on drop? As option?
    }
}

impl<const N: usize, const SZ: usize> Deref for SlabBox<N, SZ> {
    type Target = [u8; SZ];

    fn deref(&self) -> &Self::Target {
        let buf = unsafe { self.slab.get_idx_unchecked(self.idx).buf };

        unsafe { &*buf.get() }
    }
}

impl<const N: usize, const SZ: usize> DerefMut for SlabBox<N, SZ> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        let buf = unsafe { self.slab.get_idx_unchecked(self.idx).buf };

        unsafe { &mut *buf.get() }
    }
}

impl<const N: usize, const SZ: usize> SlabBox<N, SZ> {
    /// Convert the `SlabBox` into a `SlabArc`.
    ///
    /// This loses the ability to mutate the data within the allocation, but the
    /// may now be shared to multiple locations using reference counts
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
}

// SAFETY:
//
// SlabBoxes may be sent safely, as the underlying BSlab is Sync
unsafe impl<const N: usize, const SZ: usize> Send for SlabBox<N, SZ> {}
