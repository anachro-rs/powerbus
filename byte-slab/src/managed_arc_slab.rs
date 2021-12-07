//! A convenience type for abstracting over slices or `SlabSliceArc`s.

use core::ops::Deref;

/// A `ManagedArcSlab` may contain EITHER a borrowed `&[u8]` slice,
/// or a `SlabSliceArc`. `ManagedArcSlab`s implement the `Deref` trait
/// for access to the underlying data, and implement `serde`'s `Serialize`
/// and `Deserialize` traits, to allow them to be serialized as a slice of
/// bytes.
#[derive(Clone)]
pub enum ManagedArcSlab<'a, const N: usize, const SZ: usize> {
    Borrowed(&'a [u8]),
    Owned(SlabSliceArc<N, SZ>),
}


use core::fmt::Debug;
use core::marker::PhantomData;
use serde::de::Deserialize;
use serde::ser::Serialize;

use crate::{slab_arc::SlabArc, slab_slice_arc::SlabSliceArc};

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
        S: serde::Serializer,
    {
        let data: &[u8] = self.deref();
        data.serialize(serializer)
    }
}

impl<'de: 'a, 'a, const N: usize, const SZ: usize> Deserialize<'de> for ManagedArcSlab<'a, N, SZ> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct ByteVisitor<'a, const N: usize, const SZ: usize> {
            pd: PhantomData<&'a ()>,
        }

        impl<'d: 'ai, 'ai, const NI: usize, const SZI: usize> serde::de::Visitor<'d>
            for ByteVisitor<'ai, NI, SZI>
        {
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
            ManagedArcSlab::Owned(ssa) => ssa.deref(),
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
