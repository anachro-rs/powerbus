//! A reference counted, partial view of an allocation
//!
//! A `SlabSliceArc` is used to provide a view onto a portion of a `SlabArc`,
//! without sharing the entire allocation. It shares the same reference count
//! as the underlying `SlabArc`, meaning the underlying `SlabArc` will not be
//! freed if there are only `SlabSliceArc`s remaining. The underlying memory
//! is freed for reuse automatically when the reference count reaches zero.

use core::ops::Deref;

use crate::slab_arc::SlabArc;

// TODO: This doesn't HAVE to be 'static, but it makes my life easier
// if you want not-that, I guess open an issue and let me know?
/// A partial view, reference counted, BSlab allocated chunk of bytes.
///
/// `SlabSliceArc`s implement the `Deref` trait for access to the
/// underlying allocation
#[derive(Clone)]
pub struct SlabSliceArc<const N: usize, const SZ: usize> {
    pub(crate) arc: SlabArc<N, SZ>,
    pub(crate) start: usize,
    pub(crate) len: usize,
}

impl<const N: usize, const SZ: usize> Deref for SlabSliceArc<N, SZ> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        // thanks mara for the cleaner slice syntax!
        &self.arc.deref()[self.start..][..self.len]
    }
}

impl<const N: usize, const SZ: usize> SlabSliceArc<N, SZ> {
    /// Create a (smaller) `SlabSliceArc` from this `SlabSliceArc`, with a partial view
    /// of the underlying data.
    ///
    /// This function will fail if `start` and `len` do not describe a valid
    /// region of the `SlabSliceArc`.
    pub fn sub_slice_arc(&self, start: usize, len: usize) -> Result<SlabSliceArc<N, SZ>, ()> {
        let new_arc = self.arc.clone();

        // Offset inside of our own slice
        let start = self.start + start;

        let new_end = self.start + self.len;
        let good_start = start < new_end;
        let good_len = (start + len) <= new_end;

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
