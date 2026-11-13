use proc_macro2::TokenStream as TokenStream2;
use syn::{Attribute, Type};

pub fn unwrap_attr(attrs: &[Attribute], ident: &str) -> Option<TokenStream2> {
    attrs
        .iter()
        .find(|attr| attr.path().is_ident(ident))
        .and_then(|attr| {
            if let syn::Meta::List(list) = &attr.meta {
                Some(list.tokens.clone())
            } else {
                None
            }
        })
}

pub fn is_type(ty: &Type, is_type_fn: fn(&Type) -> bool) -> bool {
    if let Type::Path(type_path) = ty {
        let segments = &type_path.path.segments;
        if let Some(last) = segments.last()
            && last.ident == "Option"
            && let syn::PathArguments::AngleBracketed(args) = &last.arguments
            && let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first()
        {
            return is_type_fn(inner_ty);
        }
    }
    false
}

/// The `T` of an `Option<T>`.
pub fn option_inner(ty: &Type) -> Option<&Type> {
    if let Type::Path(type_path) = ty
        && let Some(last) = type_path.path.segments.last()
        && last.ident == "Option"
        && let syn::PathArguments::AngleBracketed(args) = &last.arguments
        && let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first()
    {
        return Some(inner_ty);
    }
    None
}

/// Check if a type is exactly PolySignal (for default_connection code generation)
pub fn is_poly_signal_type(ty: &Type) -> bool {
    match ty {
        Type::Path(tp) => tp
            .path
            .segments
            .last()
            .map(|seg| seg.ident == "PolySignal")
            .unwrap_or(false),
        _ => false,
    }
}

/// Check if a type is Option<PolySignal>
pub fn is_option_poly_signal_type(ty: &Type) -> bool {
    is_type(ty, is_poly_signal_type)
}

/// Check if a type is exactly LaneSum (for default_connection code generation)
pub fn is_mono_signal_type(ty: &Type) -> bool {
    match ty {
        Type::Path(tp) => tp
            .path
            .segments
            .last()
            .map(|seg| seg.ident == "LaneSum")
            .unwrap_or(false),
        _ => false,
    }
}

/// Check if a type is Option<LaneSum>
pub fn is_option_mono_signal_type(ty: &Type) -> bool {
    is_type(ty, is_mono_signal_type)
}

/// Check if a type is Option<Signal>
pub fn is_option_signal_type(ty: &Type) -> bool {
    is_type(ty, is_signal_type)
}

/// Check if a type is exactly Signal
pub fn is_signal_type(ty: &Type) -> bool {
    match ty {
        Type::Path(tp) => tp
            .path
            .segments
            .last()
            .map(|seg| seg.ident == "Signal")
            .unwrap_or(false),
        _ => false,
    }
}
