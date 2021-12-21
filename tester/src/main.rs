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
    T: Reroot<Retval = T> + 'static,
    Result<T, u8>: Reroot<Retval = Result<T::Retval, u8>> + 'static,
    Option<T>: Reroot<Retval = Option<T::Retval>> + 'static,
{
    foo: u8,
    bar: ManagedArcSlab<'a, N, SZ>,
    baz: T,
    bib: Result<T, u8>,
    bim: Option<T>,
}
