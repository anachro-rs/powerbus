use byte_slab::{BSlab, SlabBox, SlabArc, SlabSliceArc};
use serde::{Serialize, Deserialize};

#[derive(Deserialize)]
struct Contains {
    simple: SlabArc<16, 1024>,
}

static SLAB: BSlab<16, 1024> = BSlab::new();

fn main() {
    println!("Hello, world!");
    SLAB.init().unwrap();
}
