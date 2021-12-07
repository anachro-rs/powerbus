# Byte Slab

Byte Slab is a crate that provides a pool or slab of bytes, which can be granted in
fixed-size chunks. It is similar to heapless::Pool, however it also allows conversion
of the allocations (`SlabBox`es) into shared, reference counted objects (`SlabArc`s).

Currently, it maintains its free list as an MPMC queue, though that is an implementation
detail that may change. This implementation is convenient, but not particularly memory-dense.

The slab is statically allocated, and the size of each Box, as well as the total number of
Boxes available is selected through compile time `const` values.

Byte Slab is intended to provide boxes suitable for using as DMA buffers on bare metal
embedded systems without a general purpose allocator. All allocations are failable.

## Main components

The byte slab crate is made up of the following primary elements:

* `BSlab` - a Byte Slab. This struct represents the storage of all boxes and their
    related metadata.
* `SlabBox` - An owned allocation from the BSlab, which may be read or written to
    (exclusively) by the owner. A `SlabBox` may be converted into a `SlabArc`. The
    underlying memory is freed for reuse automatically when the Box has been dropped.
* `SlabArc` - A reference counted allocation from the BSlab, obtained by consuming a
    `SlabBox`. As the underlying allocation may be shared, a `SlabArc` does not allow
    for the contents to be modified. `SlabArc`s may be cloned (which increases the
    reference count), allowing for multiple (immutable) access to the same data. The
    underlying memory is freed for reuse automatically when the reference count reaches
    zero.
* `SlabSliceArc` - a reference counted view of a `SlabArc`. This is used to provide a
    view onto a portion of a `SlabArc`, without sharing the entire allocation. It shares
    the same reference count as the underlying `SlabArc`, meaning the underlying `SlabArc`
    will not be freed if there are only `SlabSliceArc`s remaining. The underlying memory
    is freed for reuse automatically when the reference count reaches zero.
* `ManagedArcSlab` - a convenience type that may contain EITHER a borrowed `&[u8]` slice,
    or a `SlabSliceArc`.
