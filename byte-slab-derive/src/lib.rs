use synstructure::{quote, BindStyle};
use proc_macro2::{TokenStream, Ident, Span};
use quote::ToTokens;
use syn::GenericParam;

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

    let mut gen_fixed = TokenStream::new();

    for gen in gens.params.iter() {
        match gen {
            GenericParam::Type(tp) => quote! { #tp, },
            GenericParam::Lifetime(ltd) => quote! { #ltd, },
            GenericParam::Const(cp) => {
                let ident = &cp.ident;
                quote! { #ident, }
            }
        }.to_tokens(&mut gen_fixed);
    }

    quote!{
        #ident < #gen_fixed >
    }.to_tokens(&mut ret_val);

    let mut body = TokenStream::new();

    for var in s.variants().iter() {
        let pat = var.pat();
        let cons = var.construct(|_field, i| {
            let binding = var.bindings().iter().nth(i).unwrap();
            quote!{
                #binding.reroot(key)?
            }
        });

        quote! {
            #pat => Ok(#cons),
        }.to_tokens(&mut body);
    }

    s.gen_impl(quote! {
        extern crate byte_slab;

        gen impl byte_slab::Reroot for @Self {
            type Retval = #ret_val;
            #[inline]
            fn reroot(self, key: &byte_slab::RerooterKey) -> Result<Self::Retval, ()> {
                match self {
                    #body
                }
            }
        }
    })
}

synstructure::decl_derive!([Reroot] => reroot_derive);
