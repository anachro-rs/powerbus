use synstructure::{quote, BindStyle, BindingInfo};
use proc_macro2::TokenStream;
use quote::{ToTokens, TokenStreamExt};



fn reroot_derive(mut s: synstructure::Structure) -> proc_macro2::TokenStream {
    s.bind_with(|_| {
        BindStyle::Move
    });
    // let body = s.each(|bi| {
    //     let x = bi.ast();
    //     let ident = x.ident.as_ref().unwrap();
    //     quote!{
    //         #ident: #ident.reroot(arc)?,
    //     }
    // });

    let mut body = TokenStream::new();

    for var in s.variants().iter() {
        let id = var.ast().ident;


        let mut fields = TokenStream::new();
        let mut fields_root = TokenStream::new();

        for field in var.ast().fields.iter() {
            let y = field.ident.as_ref();

            quote! {
                #y,
            }.to_tokens(&mut fields);

            quote! {
                #y: #y.reroot(arc)?,
            }.to_tokens(&mut fields_root);
        }

        quote! {
            #id { #fields } => {
                Ok(#id { #fields_root })
            }
        }.to_tokens(&mut body);
    }

    // panic!("{}", body.to_string());

    // let lol = umm.iter().map(|t| quote! { #t }).collect::<Vec<_>>();

    // panic!("\n{:?}\n", lol);

    // let mut body = TokenStream::new();

    // s.variants_mut().iter_mut().for_each(|mut v| {
    //     v.to_tokens(&mut body);
    //     v.bindings_mut().iter_mut().for_each(|mut b| {
    //         b.style = BindStyle::RefMut;
    //         b.to_tokens(&mut body);
    //     })
    // });

    panic!("\n\n\n{}\n\n\n", body.to_string());

    // impl<'a, const N: usize, const SZ: usize> Reroot<N, SZ> for ManagedArcStr<'a, N, SZ> {
    //     type Retval = ManagedArcStr<'static, N, SZ>;

    //     fn reroot(self, arc: &SlabArc<N, SZ>) -> Result<Self::Retval, ()> {
    //         self.rerooter(arc).ok_or(())
    //     }
    // }

    s.gen_impl(quote! {
        extern crate byte_slab;

        gen impl<const N: usize, const SZ: usize> byte_slab::Reroot<N, SZ> for @Self {
            fn reroot(self, arc: &byte_slab::SlabArc<N, SZ>) -> Result<Self::Retval, ()> {
                match self {
                    #body
                }
            }
        }
    })
}

synstructure::decl_derive!([Reroot] => reroot_derive);
