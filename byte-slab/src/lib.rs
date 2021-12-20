//! # Byte Slab
//!
//! Byte Slab is a crate that provides a pool or slab of bytes, which can be granted in
//! fixed-size chunks. It is similar to heapless::Pool, however it also allows conversion
//! of the allocations (`SlabBox`es) into shared, reference counted objects (`SlabArc`s).
//!
//! Currently, it maintains its free list as an MPMC queue, though that is an implementation
//! detail that may change. This implementation is convenient, but not particularly memory-dense.
//!
//! The slab is statically allocated, and the size of each Box, as well as the total number of
//! Boxes available is selected through compile time `const` values.
//!
//! Byte Slab is intended to provide boxes suitable for using as DMA buffers on bare metal
//! embedded systems without a general purpose allocator. All allocations are failable.
//!
//! ## Main components
//!
//! The byte slab crate is made up of the following primary elements:
//!
//! * `BSlab` - a Byte Slab. This struct represents the storage of all boxes and their
//!     related metadata.
//! * `SlabBox` - An owned allocation from the BSlab, which may be read or written to
//!     (exclusively) by the owner. A `SlabBox` may be converted into a `SlabArc`. The
//!     underlying memory is freed for reuse automatically when the Box has been dropped.
//! * `SlabArc` - A reference counted allocation from the BSlab, obtained by consuming a
//!     `SlabBox`. As the underlying allocation may be shared, a `SlabArc` does not allow
//!     for the contents to be modified. `SlabArc`s may be cloned (which increases the
//!     reference count), allowing for multiple (immutable) access to the same data. The
//!     underlying memory is freed for reuse automatically when the reference count reaches
//!     zero.
//! * `SlabSliceArc` - a reference counted view of a `SlabArc`. This is used to provide a
//!     view onto a portion of a `SlabArc`, without sharing the entire allocation. It shares
//!     the same reference count as the underlying `SlabArc`, meaning the underlying `SlabArc`
//!     will not be freed if there are only `SlabSliceArc`s remaining. The underlying memory
//!     is freed for reuse automatically when the reference count reaches zero.
//! * `ManagedArcSlab` - a convenience type that may contain EITHER a borrowed `&[u8]` slice,
//!     or a `SlabSliceArc`.
//!
//! ## Example
//!
//! ```rust
//! use byte_slab::BSlab;
//!
//! // Declare a byte slab with four elements, each 128 bytes
//! static SLAB: BSlab<4, 128> = BSlab::new();
//!
//! fn main() {
//!     // Initialize the byte slab
//!     SLAB.init().unwrap();
//!
//!     // Get the first box
//!     let mut box_1 = SLAB.alloc_box().unwrap();
//!
//!     assert_eq!(box_1.len(), 128);
//!     box_1.iter_mut().for_each(|i| *i = 42);
//!
//!     // We can also get three more boxes
//!     let mut box_2 = SLAB.alloc_box().unwrap();
//!     let mut box_3 = SLAB.alloc_box().unwrap();
//!     let mut box_4 = SLAB.alloc_box().unwrap();
//!
//!     // Uh oh, no more boxes!
//!     assert!(SLAB.alloc_box().is_none());
//!
//!     // Until we free one!
//!     drop(box_2);
//!
//!     // Then we can grab one again
//!     let mut box_4 = SLAB.alloc_box().unwrap();
//! }
//! ```
//!
//! ## Safety
//!
//! This probably does not handle unwind safety correctly!
//! Please verify before using in non-abort-panic environments!

#![cfg_attr(not(test), no_std)]

pub mod byte_slab;
pub mod slab_arc;
pub mod slab_box;
pub mod slab_slice_arc;
pub mod managed_arc_slab;

pub use crate::{
    byte_slab::BSlab,
    slab_arc::{SlabArc, RerooterKey},
    slab_box::SlabBox,
    slab_slice_arc::{SlabSliceArc, SlabStrArc},
    managed_arc_slab::{ManagedArcSlab, ManagedArcStr, Reroot},
};

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
