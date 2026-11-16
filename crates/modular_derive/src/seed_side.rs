use proc_macro::TokenStream;
use quote::quote;
use syn::spanned::Spanned;
use syn::{Data, DeriveInput};

/// `#[derive(SeedSide)]`: a new side copies the left side's state, except
/// fields marked `#[seed_side(keep)]`, which keep the new side's own value.
/// The type must be `Copy`, so the copy cannot allocate.
pub fn impl_seed_side_macro(ast: &DeriveInput) -> TokenStream {
    let name = &ast.ident;
    let Data::Struct(data) = &ast.data else {
        return syn::Error::new(ast.span(), "#[derive(SeedSide)] only supports structs")
            .to_compile_error()
            .into();
    };
    let mut kept = Vec::new();
    for field in data.fields.iter() {
        for attr in field
            .attrs
            .iter()
            .filter(|a| a.path().is_ident("seed_side"))
        {
            let keep = attr.parse_args::<syn::Ident>().is_ok_and(|i| i == "keep");
            let error = match (&field.ident, keep) {
                (Some(ident), true) => {
                    kept.push(ident.clone());
                    continue;
                }
                (None, true) => "#[seed_side(keep)] needs a named field",
                (_, false) => "expected #[seed_side(keep)]",
            };
            return syn::Error::new(attr.span(), error)
                .to_compile_error()
                .into();
        }
    }
    let mut generics = ast.generics.clone();
    generics
        .make_where_clause()
        .predicates
        .push(syn::parse_quote!(Self: Copy));
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    quote! {
        impl #impl_generics crate::types::SeedSide for #name #ty_generics #where_clause {
            fn seed_side(&mut self, left: &Self) {
                #( let #kept = self.#kept; )*
                *self = *left;
                #( self.#kept = #kept; )*
            }
        }
    }
    .into()
}
