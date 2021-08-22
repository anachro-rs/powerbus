# Okay, this is what I WANT to be possible

```rust
use byte_slab::{BSlab, SlabBox, SlabArc, SlabSliceArc};
use serde::{Serialize, Deserialize};

static SLAB: BSlab<16, 1024> = BSlab::new();

enum SlabManaged<'a, const N: usize, const SZ: usize> {
    Borrowed(&[u8]),
    Owned(SlabArcSlice<N, SZ>),
}

#[derive(Serialize, Deserialize)]
struct Example<'a, const N: usize, const SZ: usize> {
    x: u32,
    first: SlabManaged<'a, N, SZ>,
    y: bool,
    second: SlabManaged<'a, N, SZ>,
}

fn main() {
    let initial = Example {
        x: 42,
        first: SlabManaged::Borrowed(&[1, 2, 3]),
        y: true,
        second: SlabManaged::Borrowed(&[10, 20, 30, 40]),
    };

    let ser_box: SlabBox = ser_to_box(&initial).unwrap();
    let ser_arc: SlabArc = ser_box.into_arc();

    // NOTE: Does NOT allocate any new storage! Increases
    // ref-counts from `ser_arc`.
    // `first` and `second` are both `SlabManaged::Owned`.
    let deser = deser_from_arc::<Example>(&ser_arc).unwrap();

    // Drop the original, but the parts are still okay!
    drop(ser_arc);

    assert_eq!(initial, deser);
}
```
