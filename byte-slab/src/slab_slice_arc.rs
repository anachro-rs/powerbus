//! A reference counted, partial view of an allocation
//!
//! A `SlabSliceArc` is used to provide a view onto a portion of a `SlabArc`,
//! without sharing the entire allocation. It shares the same reference count
//! as the underlying `SlabArc`, meaning the underlying `SlabArc` will not be
//! freed if there are only `SlabSliceArc`s remaining. The underlying memory
//! is freed for reuse automatically when the reference count reaches zero.

use core::ops::Deref;

use crate::slab_arc::SlabArc;
use core::str::{from_utf8_unchecked, from_utf8, Utf8Error};

// TODO: This doesn't HAVE to be 'static, but it makes my life easier
// if you want not-that, I guess open an issue and let me know?
/// A partial view, reference counted, BSlab allocated chunk of bytes.
///
/// `SlabSliceArc`s implement the `Deref` trait for access to the
/// underlying allocation
///
/// ## Example
///
/// ```rust
/// use byte_slab::BSlab;
/// use std::thread::spawn;
///
/// static SLAB: BSlab<4, 128> = BSlab::new();
///
/// fn main() {
///     // Initialize the byte slab
///     SLAB.init().unwrap();
///
///     let mut box_1 = SLAB.alloc_box().unwrap();
///
///     // Fill
///     assert_eq!(box_1.len(), 128);
///     box_1.iter_mut().enumerate().for_each(|(i, x)| *x = i as u8);
///
///     // Convert the Box into an Arc for sharing
///     let arc_1 = box_1.into_arc();
///
///     // And we can cheaply take a subslice of the parent
///     let sub_arc_1 = arc_1.sub_slice_arc(64, 64).unwrap();
///
///     // We can now send the sub-slice arc to another thread
///     let hdl = spawn(move || {
///         assert_eq!(sub_arc_1.len(), 64);
///         sub_arc_1.iter().enumerate().for_each(|(i, x)| assert_eq!(i as u8 + 64, *x));
///     });
///
///     // ... while still retaining a local handle to the same data
///     arc_1.iter().enumerate().for_each(|(i, x)| assert_eq!(i as u8, *x));
///
///     hdl.join();
/// }
/// ```
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

    /// Convert the current `SlabSliceArc` into a `SlabStrArc`.
    ///
    /// If the contained bytes do not constitute a valid UTF-8 string, an
    /// error will be returned.
    ///
    /// Note, regardless of success, this function will consume the Slab Arc.
    /// If this is not desired, consider using [SlabStrArc::from_slab_slice()](SlabStrArc::from_slab_slice())
    /// instead.
    pub fn into_str_arc(self) -> Result<SlabStrArc<N, SZ>, Utf8Error> {
        let _str = from_utf8(&self)?;
        Ok(SlabStrArc { inner: self })
    }
}

/// A partial view, reference counted, BSlab allocated string slice
///
/// `SlabStrArc`s implement the `Deref` trait for access to the
/// underlying allocation as a &str.
///
/// ## Example
///
/// ```rust
/// use byte_slab::BSlab;
/// use std::thread::spawn;
/// use core::ops::Deref;
///
/// static SLAB: BSlab<4, 128> = BSlab::new();
///
/// fn main() {
///     // Initialize the byte slab
///     SLAB.init().unwrap();
///
///     let mut box_1 = SLAB.alloc_box().unwrap();
///
///     // Fill
///     assert_eq!(box_1.len(), 128);
///     let msg = "Hello, 🌍!";
///     let msg_len = msg.as_bytes().len();
///     box_1[..msg_len].copy_from_slice(msg.as_bytes());
///
///     // Convert the Box into an Arc for sharing
///     let arc_1 = box_1.into_arc();
///
///     // And we can turn this into an ArcStr
///     let sub_arc_1 = arc_1
///         .sub_slice_arc(0, msg_len)
///         .unwrap()
///         .into_str_arc()
///         .unwrap();
///
///     // We can now send the sub-slice arc to another thread
///     let hdl = spawn(move || {
///         assert_eq!(sub_arc_1.len(), msg_len);
///         assert_eq!(sub_arc_1.deref(), msg);
///     });
///
///     // ... while still retaining a local handle to the same data
///     assert_eq!(&arc_1[..msg_len], msg.as_bytes());
///
///     hdl.join();
/// }
/// ```
#[derive(Clone)]
pub struct SlabStrArc<const N: usize, const SZ: usize> {
    pub(crate) inner: SlabSliceArc<N, SZ>,
}

impl<const N: usize, const SZ: usize> PartialEq<str> for SlabStrArc<N, SZ> {
    fn eq(&self, other: &str) -> bool {
        let stir: &str = self.deref();
        stir.eq(other)
    }
}

impl<const N: usize, const SZ: usize> PartialEq for SlabStrArc<N, SZ> {
    fn eq(&self, other: &Self) -> bool {
        let stir_me: &str = self.deref();
        let stir_ot: &str = other.deref();
        stir_me.eq(stir_ot)
    }
}

impl<const N: usize, const SZ: usize> Eq for SlabStrArc<N, SZ> { }

impl<const N: usize, const SZ: usize> Deref for SlabStrArc<N, SZ> {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        let slice: &[u8] = &self.inner;

        // SAFETY: `SlabStrArc`s are checked on creation
        unsafe {
            from_utf8_unchecked(slice)
        }
    }
}

impl<const N: usize, const SZ: usize> SlabStrArc<N, SZ> {
    /// Convert the given `SlabSliceArc` into a `SlabStrArc`.
    ///
    /// If the contained bytes do not constitute a valid UTF-8 string, an
    /// error will be returned. Unlike [SlabSliceArc::into_str_arc()](SlabSliceArc::into_str_arc()),
    /// this function does not consume the slice, instead cloning the underlying Arc.
    pub fn from_slab_slice(other: &SlabSliceArc<N, SZ>) -> Result<Self, Utf8Error> {
        let clone = other.clone();
        clone.into_str_arc()
    }
}
