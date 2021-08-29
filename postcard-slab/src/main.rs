use byte_slab::{BSlab, SlabBox, SlabArc, SlabSliceArc};
use serde::{Deserialize, Deserializer, Serialize};

#[derive(Deserialize)]
struct Contains {
    #[serde(deserialize_with = "not_right")]
    simple: SlabArc<16, 1024>,
}

static SLAB: BSlab<16, 1024> = BSlab::new();

fn main() {
    println!("Hello, world!");
    SLAB.init().unwrap();
}

fn not_right<'de, D, T>(arg: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
{
    todo!()
}
