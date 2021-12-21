//! A convenience type for abstracting over slices or `SlabSliceArc`s.

use core::{
    ops::Deref,
    fmt::Debug,
    marker::PhantomData,
};
use serde::{Deserialize, Serialize};

use crate::{
    byte_slab::BSlab,
    slab_arc::{SlabArc, RerooterKey},
    slab_slice_arc::{SlabSliceArc, SlabStrArc}
};

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

#[cfg(feature = "defmt")]
impl<'a, const N: usize, const SZ: usize> defmt::Format for ManagedArcSlab<'a, N, SZ> {
    fn format(&self, fmt: defmt::Formatter<'_>) {
        self.deref().format(fmt)
    }
}

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

            fn expecting(&self, formatter: &mut core::fmt::Formatter) -> core::fmt::Result {
                write!(formatter, "a byte slice")
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

    pub fn reroot_with_key(self, key: &RerooterKey) -> Option<ManagedArcSlab<'static, N, SZ>> {
        match self {
            ManagedArcSlab::Owned(e) => Some(ManagedArcSlab::Owned(e)),
            ManagedArcSlab::Borrowed(b) => {
                if b.is_empty() {
                    // TODO: nuance
                    return None;
                }

                // TODO: yolo ub
                let start: usize = key.start as usize;
                let end: usize = key.end as usize;
                let b_start: usize = b.as_ptr() as usize;

                if (start <= b_start) && (b_start < end) {
                    let bslab: &'static BSlab<N, SZ> = unsafe { &*key.slab.cast::<BSlab<N, SZ>>() };

                    // NOTE: We *don't* increase the refcount for this arc, as it technically aliases the
                    // borrowed one that created the `key` here. Since we will forget this arc after
                    // creating the ssa, don't update!
                    let arc = SlabArc {
                        slab: bslab,
                        idx: key.idx,
                    };

                    let ssa = arc
                        .sub_slice_arc(b_start - start, b.len())
                        .ok()?;

                    // Okay, now forget that arc ever happened
                    core::mem::forget(arc);

                    Some(ManagedArcSlab::Owned(ssa))
                } else {
                    None
                }
            }
        }
    }
}

/// A `ManagedArcStr` may contain EITHER a borrowed `&str` slice,
/// or a `SlabStrArc`. `ManagedArcStr`s implement the `Deref` trait
/// for access to the underlying data, and implement `serde`'s `Serialize`
/// and `Deserialize` traits, to allow them to be serialized as a string slice.
#[derive(Clone)]
pub enum ManagedArcStr<'a, const N: usize, const SZ: usize> {
    Borrowed(&'a str),
    Owned(SlabStrArc<N, SZ>),
}


impl<'a, const N: usize, const SZ: usize> PartialEq<str> for ManagedArcStr<'a, N, SZ> {
    fn eq(&self, other: &str) -> bool {
        let stir: &str = self.deref();
        stir.eq(other)
    }
}

impl<'a, const N: usize, const SZ: usize> PartialEq for ManagedArcStr<'a, N, SZ> {
    fn eq(&self, other: &Self) -> bool {
        let stir_me: &str = self.deref();
        let stir_ot: &str = other.deref();
        stir_me.eq(stir_ot)
    }
}

impl<'a, const N: usize, const SZ: usize> Eq for ManagedArcStr<'a, N, SZ> { }

#[cfg(feature = "defmt")]
impl<'a, const N: usize, const SZ: usize> defmt::Format for ManagedArcStr<'a, N, SZ> {
    fn format(&self, fmt: defmt::Formatter<'_>) {
        self.deref().format(fmt)
    }
}

impl<'a, const N: usize, const SZ: usize> Debug for ManagedArcStr<'a, N, SZ> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // TODO: Probably want a better debug impl than this
        match self {
            ManagedArcStr::Borrowed(b) => b.fmt(f),
            ManagedArcStr::Owned(o) => o.deref().fmt(f),
        }
    }
}

impl<'a, const N: usize, const SZ: usize> Serialize for ManagedArcStr<'a, N, SZ> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let data: &str = self.deref();
        data.serialize(serializer)
    }
}

impl<'de: 'a, 'a, const N: usize, const SZ: usize> Deserialize<'de> for ManagedArcStr<'a, N, SZ> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct StrVisitor<'a, const N: usize, const SZ: usize> {
            pd: PhantomData<&'a ()>,
        }

        impl<'d: 'ai, 'ai, const NI: usize, const SZI: usize> serde::de::Visitor<'d>
            for StrVisitor<'ai, NI, SZI>
        {
            type Value = ManagedArcStr<'ai, NI, SZI>;

            fn expecting(&self, formatter: &mut core::fmt::Formatter) -> core::fmt::Result {
                write!(formatter, "a byte slice")
            }

            fn visit_borrowed_str<E>(self, v: &'d str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(ManagedArcStr::Borrowed(v))
            }
        }
        deserializer.deserialize_str(StrVisitor { pd: PhantomData })
    }
}

impl<'a, const N: usize, const SZ: usize> Deref for ManagedArcStr<'a, N, SZ> {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        match self {
            ManagedArcStr::Borrowed(data) => data,
            ManagedArcStr::Owned(ssa) => ssa.deref(),
        }
    }
}

impl<'a, const N: usize, const SZ: usize> ManagedArcStr<'a, N, SZ> {
    pub fn from_slice(sli: &'a str) -> ManagedArcStr<'a, N, SZ> {
        ManagedArcStr::Borrowed(sli)
    }

    pub fn from_slab_str_arc(arc: &SlabStrArc<N, SZ>) -> ManagedArcStr<'static, N, SZ> {
        ManagedArcStr::Owned(arc.clone())
    }

    pub fn reroot_with_key(self, key: &RerooterKey) -> Option<ManagedArcStr<'static, N, SZ>> {
        match self {
            ManagedArcStr::Owned(e) => Some(ManagedArcStr::Owned(e)),
            ManagedArcStr::Borrowed(b) => {
                if b.is_empty() {
                    // TODO: nuance
                    return None;
                }

                // TODO: yolo ub
                let start: usize = key.start as usize;
                let end: usize = key.end as usize;
                let b_start: usize = b.as_ptr() as usize;

                if (start <= b_start) && (b_start < end) {
                    let bslab: &'static BSlab<N, SZ> = unsafe { &*key.slab.cast::<BSlab<N, SZ>>() };

                    // NOTE: We *don't* increase the refcount for this arc, as it technically aliases the
                    // borrowed one that created the `key` here. Since we will forget this arc after
                    // creating the ssa, don't update!
                    let arc = SlabArc {
                        slab: bslab,
                        idx: key.idx,
                    };

                    let ssa = arc
                        .sub_slice_arc(b_start - start, b.len())
                        .ok()?
                        .into_str_arc()
                        .ok()?;

                    // Okay, now forget that arc ever happened
                    core::mem::forget(arc);

                    Some(ManagedArcStr::Owned(ssa))
                } else {
                    None
                }
            }
        }
    }
}

/// A trait for restoring Managed types to a given SlabArc
///
/// For types that are not Managed, this trait should just return
/// the type as-is.
pub trait Reroot
{
    /// The return type. This is used to change the lifetimes, e.g. to convert
    /// `Self: ManagedArcSlab<'a, _, _>` to `Retval: ManagedArcSlab<'static, _, _>`.
    type Retval;

    fn reroot(self, key: &RerooterKey) -> Result<Self::Retval, ()>;
}

impl<'a, const N: usize, const SZ: usize> Reroot for ManagedArcSlab<'a, N, SZ> {
    type Retval = ManagedArcSlab<'static, N, SZ>;

    fn reroot(self, key: &RerooterKey) -> Result<Self::Retval, ()>
    {
        self.reroot_with_key(key).ok_or(())
    }
}

impl<'a, const N: usize, const SZ: usize> Reroot for ManagedArcStr<'a, N, SZ> {
    type Retval = ManagedArcStr<'static, N, SZ>;

    fn reroot(self, key: &RerooterKey) -> Result<Self::Retval, ()>
    {
        self.reroot_with_key(key).ok_or(())
    }
}

macro_rules! reroot_nop {
    (
        [$($rrty:ty),+]
    ) => {
        $(
            impl Reroot for $rrty {
                type Retval = $rrty;

                #[inline(always)]
                fn reroot(self, _key: &RerooterKey) -> Result<Self::Retval, ()>
                {
                    Ok(self)
                }
            }
        )+
    };
}

reroot_nop!([u8, u16, u32, u64]);
reroot_nop!([i8, i16, i32, i64]);
reroot_nop!([bool, char, ()]);
reroot_nop!([f32, f64]);

#[cfg(test)]
mod test {
    use crate::{BSlab, ManagedArcSlab, Reroot};
    use std::ops::Deref;

    #[test]
    fn smoke() {
        static SLAB: BSlab<4, 128> = BSlab::new();
        SLAB.init().unwrap();

        let mut sbox = SLAB.alloc_box().unwrap();

        sbox[..4].copy_from_slice(&[1, 2, 3, 4]);

        let arc_1 = sbox.into_arc();

        let brw = ManagedArcSlab::<4, 128>::Borrowed(&arc_1[..4]);
        let own: ManagedArcSlab<'static, 4, 128> = brw.reroot(&arc_1).unwrap();

        match own {
            ManagedArcSlab::Owned(ssa) => {
                assert_eq!(&[1, 2, 3, 4], ssa.deref());
            }
            _ => panic!("Not owned!"),
        }
    }

}
