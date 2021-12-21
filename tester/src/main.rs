use byte_slab_derive::Reroot;
use byte_slab::{ManagedArcSlab, Reroot};

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

#[derive(Reroot)]
struct DemoComboSConst<'a, T, const N: usize, const SZ: usize>
where
    T: Reroot<Retval = T> + 'static
{
    foo: u8,
    bar: ManagedArcSlab<'a, N, SZ>,
    baz: T,
}
