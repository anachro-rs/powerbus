use synstructure::{quote, BindStyle};
use proc_macro2::{TokenStream, Ident, Span};
use quote::ToTokens;



fn reroot_derive(mut s: synstructure::Structure) -> proc_macro2::TokenStream {
    s.bind_with(|_| {
        BindStyle::Move
    });

    let mut ret_val = TokenStream::new();

    let mut sname = s.ast().clone();
    sname.generics.lifetimes_mut().for_each(|lt| {
        lt.lifetime.ident = Ident::new("static", Span::call_site());
    });

    let ident = sname.ident;
    let gens = sname.generics;

    quote!{
        #ident #gens
    }.to_tokens(&mut ret_val);

    // panic!("{}", ret_val.to_string());

    let mut body = TokenStream::new();

    for var in s.variants().iter() {
        let pat = var.pat();
        let cons = var.construct(|_field, i| {
            let binding = var.bindings().iter().nth(i).unwrap();
            quote!{
                byte_slab::Reroot::<N, SZ>::reroot(#binding, arc)?
            }
        });

        quote! {
            #pat => Ok(#cons),
        }.to_tokens(&mut body);
    }

    s.gen_impl(quote! {
        extern crate byte_slab;

        gen impl<const N: usize, const SZ: usize> byte_slab::Reroot<N, SZ> for @Self {
            type Retval = #ret_val;
            fn reroot(self, arc: &byte_slab::SlabArc<N, SZ>) -> Result<Self::Retval, ()> {
                match self {
                    #body
                }
            }
        }
    })
}

synstructure::decl_derive!([Reroot] => reroot_derive);
