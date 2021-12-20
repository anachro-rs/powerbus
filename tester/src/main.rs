use byte_slab_derive::Reroot;
use byte_slab::ManagedArcSlab;

fn main() {
    println!("Hello, world!");
}

#[derive(Reroot)]
struct DemoSimpleS {
    foo: u8,
    bar: u16,
}

#[derive(Reroot)]
enum DemoSimpleE {
    Foo(u8),
    Bar(u16),
}

// #[derive(Reroot)]
// enum DemoSimpleBad<'a> {
//     Foo(&'a str),
//     Bar(u16),
// }

#[derive(Reroot)]
struct DemoComboS<'a> {
    foo: u8,
    bar: ManagedArcSlab<'a, 4, 128>,
}
