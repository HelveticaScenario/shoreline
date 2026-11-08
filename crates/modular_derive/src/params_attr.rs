//! `#[params]`: the single place a module param is documented.
//!
//! Every visible field of a params struct carries
//! `#[input("description", type = …, default = …, range = (…), control_range = (…))]`.
//! A signal whose default depends on other params also gives `default_doc`, the
//! default in words (e.g. `"0 if decay is set, otherwise 5"`).
//! The attribute expands before the struct's derives, so it rewrites each
//! `#[input]` into the `#[doc]` (and `#[schemars(range)]`) that `JsonSchema`
//! reads, and emits the struct's `SignalParamMeta` impl and `<FIELD>_DEFAULT`
//! constants from the same data.

use convert_case::{Case, Casing};
use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::{format_ident, quote};
use syn::ext::IdentExt;
use syn::parse::ParseStream;
use syn::spanned::Spanned;
use syn::{Attribute, Fields, Ident, ItemStruct, LitFloat, LitStr, Token, Type};

use crate::utils::{
    is_mono_signal_type, is_option_mono_signal_type, is_option_poly_signal_type,
    is_poly_signal_type,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum SignalType {
    Audio,
    Control,
    Pitch,
    Frequency,
    Gate,
    Trig,
}

impl SignalType {
    fn parse(ident: &Ident) -> syn::Result<Self> {
        Ok(match ident.to_string().as_str() {
            "audio" => Self::Audio,
            "control" => Self::Control,
            "pitch" => Self::Pitch,
            "frequency" => Self::Frequency,
            "gate" => Self::Gate,
            "trig" => Self::Trig,
            other => {
                return Err(syn::Error::new(
                    ident.span(),
                    format!(
                        "unknown input type `{other}`; expected audio, control, pitch, frequency, gate or trig"
                    ),
                ));
            }
        })
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Audio => "audio",
            Self::Control => "control",
            Self::Pitch => "pitch",
            Self::Frequency => "frequency",
            Self::Gate => "gate",
            Self::Trig => "trig",
        }
    }
}

/// A number as written, keeping its literal for re-emission.
#[derive(Clone)]
struct Number {
    value: f64,
    tokens: TokenStream2,
}

/// `(min, max)` where either side may be `..` (unbounded).
#[derive(Clone)]
struct Bounds {
    min: Option<Number>,
    max: Option<Number>,
    span: Span,
}

impl Bounds {
    fn is_closed(&self) -> bool {
        self.min.is_some() && self.max.is_some()
    }
}

/// A documented default: a number for a signal, or any literal for a plain
/// param, where it is the value serde fills in.
enum DocumentedDefault {
    Number(Number),
    Literal(syn::Lit),
}

struct InputAttr {
    description: String,
    signal_type: Option<(SignalType, Span)>,
    default: Option<(DocumentedDefault, Span)>,
    default_doc: Option<LitStr>,
    range: Option<Bounds>,
    control_range: Option<Bounds>,
}

fn parse_number(input: ParseStream) -> syn::Result<Number> {
    let negative = input.peek(Token![-]);
    if negative {
        input.parse::<Token![-]>()?;
    }
    let lit: syn::Lit = input.parse()?;
    let value = match &lit {
        syn::Lit::Float(f) => f.base10_parse::<f64>()?,
        syn::Lit::Int(i) => i.base10_parse::<i64>()? as f64,
        _ => return Err(syn::Error::new(lit.span(), "expected a number literal")),
    };
    let tokens = if negative {
        quote! { -#lit }
    } else {
        quote! { #lit }
    };
    Ok(Number {
        value: if negative { -value } else { value },
        tokens,
    })
}

fn parse_bound(input: ParseStream) -> syn::Result<Option<Number>> {
    if input.peek(Token![..]) {
        input.parse::<Token![..]>()?;
        Ok(None)
    } else {
        parse_number(input).map(Some)
    }
}

fn parse_bounds(input: ParseStream) -> syn::Result<Bounds> {
    let content;
    let paren = syn::parenthesized!(content in input);
    let min = parse_bound(&content)?;
    content.parse::<Token![,]>()?;
    let max = parse_bound(&content)?;
    if !content.is_empty() {
        return Err(content.error("expected `(min, max)`"));
    }
    let span = paren.span.join();
    if min.is_none() && max.is_none() {
        return Err(syn::Error::new(
            span,
            "a range needs at least one bound; leave it out for an unbounded input",
        ));
    }
    if let (Some(lo), Some(hi)) = (&min, &max)
        && lo.value >= hi.value
    {
        return Err(syn::Error::new(span, "range min must be below its max"));
    }
    Ok(Bounds { min, max, span })
}

fn parse_input_attr(attr: &Attribute) -> syn::Result<InputAttr> {
    let parser = |input: ParseStream| -> syn::Result<InputAttr> {
        let description: LitStr = input.parse().map_err(|e| {
            syn::Error::new(
                e.span(),
                "`#[input]` starts with the param's description string",
            )
        })?;
        if description.value().trim().is_empty() {
            return Err(syn::Error::new(
                description.span(),
                "the input description must not be empty",
            ));
        }
        let mut parsed = InputAttr {
            description: description.value(),
            signal_type: None,
            default: None,
            default_doc: None,
            range: None,
            control_range: None,
        };
        while !input.is_empty() {
            input.parse::<Token![,]>()?;
            if input.is_empty() {
                break;
            }
            let key = Ident::parse_any(input)?;
            input.parse::<Token![=]>()?;
            let duplicate = || syn::Error::new(key.span(), format!("duplicate `{key}`"));
            match key.to_string().as_str() {
                "type" => {
                    let value = Ident::parse_any(input)?;
                    if parsed.signal_type.is_some() {
                        return Err(duplicate());
                    }
                    parsed.signal_type = Some((SignalType::parse(&value)?, value.span()));
                }
                "default" => {
                    if parsed.default.is_some() {
                        return Err(duplicate());
                    }
                    let value = if input.peek(LitStr) || input.peek(syn::LitBool) {
                        DocumentedDefault::Literal(input.parse()?)
                    } else {
                        DocumentedDefault::Number(parse_number(input)?)
                    };
                    parsed.default = Some((value, key.span()));
                }
                "default_doc" => {
                    if parsed.default_doc.is_some() {
                        return Err(duplicate());
                    }
                    parsed.default_doc = Some(input.parse()?);
                }
                "range" => {
                    if parsed.range.is_some() {
                        return Err(duplicate());
                    }
                    parsed.range = Some(parse_bounds(input)?);
                }
                "control_range" => {
                    if parsed.control_range.is_some() {
                        return Err(duplicate());
                    }
                    parsed.control_range = Some(parse_bounds(input)?);
                }
                other => {
                    return Err(syn::Error::new(
                        key.span(),
                        format!(
                            "unknown `#[input]` key `{other}`; expected type, default, default_doc, range or control_range"
                        ),
                    ));
                }
            }
        }
        Ok(parsed)
    };
    attr.parse_args_with(parser)
}

fn is_signal_field(ty: &Type) -> bool {
    is_poly_signal_type(ty)
        || is_mono_signal_type(ty)
        || is_option_poly_signal_type(ty)
        || is_option_mono_signal_type(ty)
}

const NUMERIC_TYPES: &[&str] = &[
    "u8", "u16", "u32", "u64", "usize", "i8", "i16", "i32", "i64", "isize", "f32", "f64",
];

/// A plain number, or `Option` of one.
fn is_numeric_field(ty: &Type) -> bool {
    let Type::Path(path) = ty else {
        return false;
    };
    let Some(last) = path.path.segments.last() else {
        return false;
    };
    if last.ident == "Option" {
        if let syn::PathArguments::AngleBracketed(args) = &last.arguments
            && let Some(syn::GenericArgument::Type(inner)) = args.args.first()
        {
            return is_numeric_field(inner);
        }
        return false;
    }
    NUMERIC_TYPES.iter().any(|t| last.ident == t)
}

/// Whether `#[schemars(skip)]` or `#[serde(skip)]` hides the field from the
/// schema. Such a field is not a patch param, so it takes no `#[input]`.
fn is_schema_hidden(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        if !attr.path().is_ident("schemars") && !attr.path().is_ident("serde") {
            return false;
        }
        let mut skip = false;
        let _ = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("skip") || meta.path.is_ident("skip_deserializing") {
                skip = true;
            }
            if meta.input.peek(Token![=]) {
                let _: syn::Expr = meta.value()?.parse()?;
            } else if meta.input.peek(syn::token::Paren) {
                let _content;
                syn::parenthesized!(_content in meta.input);
            }
            Ok(())
        });
        skip
    })
}

fn option_f64(n: &Option<Number>) -> TokenStream2 {
    match n {
        Some(n) => {
            let v = n.value;
            quote! { Some(#v) }
        }
        None => quote! { None },
    }
}

/// An `f32` literal spelling `value` as written, so it rounds once.
fn f32_literal(value: f64) -> TokenStream2 {
    let lit = LitFloat::new(&format!("{:?}f32", value.abs()), Span::call_site());
    if value.is_sign_negative() {
        quote! { -#lit }
    } else {
        quote! { #lit }
    }
}

fn check_within(range: &Bounds, value: f64, span: Span, what: &str) -> syn::Result<()> {
    let below = range.min.as_ref().is_some_and(|m| value < m.value);
    let above = range.max.as_ref().is_some_and(|m| value > m.value);
    if below || above {
        return Err(syn::Error::new(
            span,
            format!("{what} lies outside `range`"),
        ));
    }
    Ok(())
}

/// Validate one field's `#[input]` against its type.
fn validate(attr: &InputAttr, field: &syn::Field) -> syn::Result<()> {
    let signal = is_signal_field(&field.ty);
    if let Some(doc) = &attr.default_doc {
        if !signal {
            return Err(syn::Error::new(
                doc.span(),
                "`default_doc` applies to signal inputs only",
            ));
        }
        if attr.default.is_none() {
            return Err(syn::Error::new(
                doc.span(),
                "`default_doc` describes a declared `default`",
            ));
        }
        if let Some((SignalType::Audio | SignalType::Gate, span)) = attr.signal_type {
            return Err(syn::Error::new(
                span,
                "`default_doc` applies to control, pitch and frequency inputs, whose defaults the docs show",
            ));
        }
    }
    if !signal {
        if let Some((_, span)) = attr.signal_type {
            return Err(syn::Error::new(
                span,
                "`type` applies to signal inputs only",
            ));
        }
        if let Some(cr) = &attr.control_range {
            return Err(syn::Error::new(
                cr.span,
                "`control_range` applies to `type = control` inputs only",
            ));
        }
        if let Some(range) = &attr.range
            && !is_numeric_field(&field.ty)
        {
            return Err(syn::Error::new(range.span, "`range` needs a numeric field"));
        }
        if let (Some(range), Some((DocumentedDefault::Number(default), span))) =
            (&attr.range, &attr.default)
        {
            check_within(range, default.value, *span, "`default`")?;
        }
        return Ok(());
    }
    let Some((signal_type, type_span)) = attr.signal_type else {
        return Err(syn::Error::new(
            field.span(),
            "signal inputs need a `type` (audio, control, pitch, frequency, gate or trig)",
        ));
    };
    match signal_type {
        SignalType::Gate | SignalType::Trig => {
            if let Some(b) = attr.range.as_ref().or(attr.control_range.as_ref()) {
                return Err(syn::Error::new(
                    b.span,
                    "gate and trig inputs take no range; they read the 0–5 V gate convention",
                ));
            }
        }
        SignalType::Control => {
            if attr.control_range.is_none() && !attr.range.as_ref().is_some_and(Bounds::is_closed) {
                return Err(syn::Error::new(
                    attr.range.as_ref().map_or(type_span, |r| r.span),
                    "control inputs need a closed `range`, or a `control_range` for the slider",
                ));
            }
            if let Some(cr) = &attr.control_range
                && !cr.is_closed()
            {
                return Err(syn::Error::new(
                    cr.span,
                    "`control_range` needs both bounds",
                ));
            }
        }
        SignalType::Audio | SignalType::Pitch | SignalType::Frequency => {
            if let Some(cr) = &attr.control_range {
                return Err(syn::Error::new(
                    cr.span,
                    "`control_range` applies to `type = control` inputs only",
                ));
            }
        }
    }
    match &attr.default {
        Some((DocumentedDefault::Literal(_), span)) => {
            return Err(syn::Error::new(
                *span,
                "a signal input's `default` is a number",
            ));
        }
        Some((DocumentedDefault::Number(default), span)) => {
            if let Some(range) = &attr.range {
                check_within(range, default.value, *span, "`default`")?;
            }
        }
        None => {}
    }
    Ok(())
}

pub fn params_impl(attr: TokenStream, item: TokenStream) -> TokenStream {
    match params(attr.into(), item.into()) {
        Ok(tokens) => tokens.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

fn params(attr: TokenStream2, item: TokenStream2) -> syn::Result<TokenStream2> {
    if !attr.is_empty() {
        return Err(syn::Error::new(
            attr.span(),
            "`#[params]` takes no arguments",
        ));
    }
    let mut ast: ItemStruct = syn::parse2(item)?;
    if !ast.attrs.iter().any(|a| a.path().is_ident("derive")) {
        return Err(syn::Error::new(
            ast.ident.span(),
            "place `#[params]` above the struct's `#[derive(...)]` so it expands first",
        ));
    }
    let name = ast.ident.clone();
    let vis = ast.vis.clone();
    let Fields::Named(fields) = &mut ast.fields else {
        return Err(syn::Error::new(
            ast.ident.span(),
            "`#[params]` needs a struct with named fields",
        ));
    };

    let mut schemas = Vec::new();
    let mut defaults = Vec::new();
    for field in fields.named.iter_mut() {
        let ident = field.ident.clone().expect("named field");
        let position = field.attrs.iter().position(|a| a.path().is_ident("input"));
        if is_schema_hidden(&field.attrs) {
            if let Some(i) = position {
                return Err(syn::Error::new(
                    field.attrs[i].span(),
                    "a `#[schemars(skip)]` field is hidden from patches and takes no `#[input]`",
                ));
            }
            continue;
        }
        if let Some(doc) = field.attrs.iter().find(|a| a.path().is_ident("doc")) {
            return Err(syn::Error::new(
                doc.span(),
                "param docs live in `#[input(\"…\")]`, not `///`",
            ));
        }
        let Some(i) = position else {
            return Err(syn::Error::new(
                ident.span(),
                format!("param `{ident}` needs an `#[input(\"description\", …)]`"),
            ));
        };
        let input_attr = field.attrs.remove(i);
        if field.attrs.iter().any(|a| a.path().is_ident("input")) {
            return Err(syn::Error::new(field.span(), "one `#[input]` per param"));
        }
        let parsed = parse_input_attr(&input_attr)?;
        validate(&parsed, field)?;

        let description = &parsed.description;
        field
            .attrs
            .insert(i, syn::parse_quote! { #[doc = #description] });

        let Some((signal_type, _)) = parsed.signal_type else {
            if let Some(range) = &parsed.range {
                let parts = [
                    range.min.as_ref().map(|n| {
                        let t = &n.tokens;
                        quote! { min = #t }
                    }),
                    range.max.as_ref().map(|n| {
                        let t = &n.tokens;
                        quote! { max = #t }
                    }),
                ];
                let parts = parts.iter().flatten();
                field
                    .attrs
                    .push(syn::parse_quote! { #[schemars(range(#(#parts),*))] });
            }
            if let Some((default, _)) = &parsed.default {
                let value = match default {
                    DocumentedDefault::Number(n) => n.tokens.clone(),
                    DocumentedDefault::Literal(lit) => quote! { #lit },
                };
                // schemars would also write `#[serde(default)]`'s value
                // (`null` for an `Option`); the skip stops that, leaving the
                // documented value.
                field.attrs.push(syn::parse_quote! {
                    #[serde(skip_serializing_if = "crate::types::documented_schema_default")]
                });
                field.attrs.push(syn::parse_quote! {
                    #[schemars(transform = |schema: &mut schemars::Schema| {
                        schema.insert("default".into(), ::serde_json::json!(#value));
                    })]
                });
            }
            continue;
        };

        let field_name = ident.to_string().to_case(Case::Camel);
        let type_str = signal_type.as_str();
        let declared_default = match &parsed.default {
            Some((DocumentedDefault::Number(n), _)) => Some(n.value),
            _ => None,
        };
        let default_value = declared_default.unwrap_or(0.0);
        let default_schema = match declared_default {
            Some(v) => quote! { Some(#v) },
            None => quote! { None },
        };
        let default_doc = match &parsed.default_doc {
            Some(doc) => quote! { Some(#doc.to_string()) },
            None => quote! { None },
        };
        let (min_value, max_value) = match &parsed.range {
            Some(r) => (option_f64(&r.min), option_f64(&r.max)),
            None => (quote! { None }, quote! { None }),
        };
        let control = match signal_type {
            SignalType::Control => parsed.control_range.as_ref().or(parsed.range.as_ref()),
            _ => None,
        };
        let (control_min, control_max) = match control {
            Some(r) => (option_f64(&r.min), option_f64(&r.max)),
            None => (quote! { None }, quote! { None }),
        };
        schemas.push(quote! {
            crate::types::SignalParamSchema {
                name: #field_name.to_string(),
                description: #description.to_string(),
                signal_type: #type_str.to_string(),
                default_value: #default_schema,
                default_doc: #default_doc,
                min_value: #min_value,
                max_value: #max_value,
                control_min: #control_min,
                control_max: #control_max,
            }
        });
        if is_option_poly_signal_type(&field.ty) {
            let default = f32_literal(default_value);
            let default_name =
                format_ident!("{}_DEFAULT", ident.to_string().to_case(Case::UpperSnake));
            defaults.push(quote! {
                /// What a disconnected input reads.
                #[allow(dead_code)]
                #vis const #default_name: f32 = #default;
            });
        }
    }

    Ok(quote! {
        #ast

        impl #name {
            #(#defaults)*
        }

        impl crate::types::SignalParamMeta for #name {
            fn signal_param_schemas() -> Vec<crate::types::SignalParamSchema> {
                vec![#(#schemas,)*]
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expand(src: &str) -> Result<String, String> {
        let item: TokenStream2 = src.parse().unwrap();
        params(TokenStream2::new(), item)
            .map(|t| t.to_string())
            .map_err(|e| e.to_string())
    }

    fn err(src: &str) -> String {
        expand(src).expect_err("expected a compile error")
    }

    #[test]
    fn rewrites_input_into_doc_and_schema() {
        let out = expand(
            r#"#[derive(Clone)] struct P {
                #[input("cutoff", type = frequency, default = 0.0, range = (-5.0, 5.0))]
                cutoff: PolySignal,
                #[input("resonance", type = control, range = (0.0, ..), control_range = (0.0, 5.0))]
                res: Option<PolySignal>,
                #[input("waves", range = (1, 16))]
                waves: u32,
            }"#,
        )
        .unwrap();
        assert!(out.contains("doc = \"cutoff\""));
        assert!(out.contains("schemars (range (min = 1 , max = 16))"));
        assert!(out.contains("RES_DEFAULT"));
        assert!(
            out.contains("control_min : Some (0f64)") || out.contains("control_min : Some (0.0")
        );
        assert!(!out.contains("# [input"));
    }

    #[test]
    fn rejects_doc_comments() {
        assert!(
            err(
                "#[derive(Clone)] struct P { /// hi\n #[input(\"x\", type = audio)] a: PolySignal }"
            )
            .contains("not `///`")
        );
    }

    #[test]
    fn requires_description_and_input() {
        assert!(err("#[derive(Clone)] struct P { a: PolySignal }").contains("needs an `#[input"));
        assert!(
            err("#[derive(Clone)] struct P { #[input(type = audio)] a: PolySignal }")
                .contains("description string")
        );
    }

    #[test]
    fn requires_signal_type() {
        assert!(
            err("#[derive(Clone)] struct P { #[input(\"a\")] a: PolySignal }")
                .contains("need a `type`")
        );
    }

    #[test]
    fn control_needs_slider_bounds() {
        assert!(
            err("#[derive(Clone)] struct P { #[input(\"a\", type = control, range = (0.0, ..))] a: PolySignal }")
                .contains("control_range")
        );
        assert!(
            expand("#[derive(Clone)] struct P { #[input(\"a\", type = control, range = (0.0, 5.0))] a: PolySignal }")
                .is_ok()
        );
    }

    #[test]
    fn control_range_only_on_control() {
        assert!(
            err("#[derive(Clone)] struct P { #[input(\"a\", type = pitch, control_range = (0.0, 5.0))] a: PolySignal }")
                .contains("control_range")
        );
        assert!(
            err("#[derive(Clone)] struct P { #[input(\"a\", range = (0, 5), control_range = (0.0, 5.0))] a: u32 }")
                .contains("control_range")
        );
    }

    #[test]
    fn gate_takes_no_range() {
        assert!(
            err("#[derive(Clone)] struct P { #[input(\"a\", type = gate, range = (0.0, 5.0))] a: PolySignal }")
                .contains("no range")
        );
    }

    #[test]
    fn default_doc_needs_a_signal_default() {
        assert!(
            err("#[derive(Clone)] struct P { #[input(\"a\", type = control, range = (0.0, 5.0), default_doc = \"x\")] a: PolySignal }")
                .contains("default_doc")
        );
        assert!(
            err("#[derive(Clone)] struct P { #[input(\"a\", default = 1, default_doc = \"x\")] a: u32 }")
                .contains("default_doc")
        );
    }

    #[test]
    fn default_within_range() {
        assert!(
            err("#[derive(Clone)] struct P { #[input(\"a\", type = control, default = 6.0, range = (0.0, 5.0))] a: PolySignal }")
                .contains("outside")
        );
        assert!(
            err(
                "#[derive(Clone)] struct P { #[input(\"a\", default = 0, range = (1, 4))] a: u32 }"
            )
            .contains("outside")
        );
    }

    #[test]
    fn plain_default_reaches_the_schema() {
        let out = expand(
            "#[derive(Clone)] struct P { #[input(\"mode\", default = \"tanh\")] mode: Drive }",
        )
        .unwrap();
        assert!(out.contains("json ! (\"tanh\")"), "{out}");
        assert!(
            err("#[derive(Clone)] struct P { #[input(\"a\", type = control, range = (0.0, 5.0), default = \"x\")] a: PolySignal }")
                .contains("is a number")
        );
    }

    #[test]
    fn range_needs_number_on_plain_fields() {
        assert!(
            err("#[derive(Clone)] struct P { #[input(\"a\", range = (0, 5))] a: bool }")
                .contains("numeric")
        );
    }

    #[test]
    fn hidden_fields_take_no_input() {
        assert!(
            expand("#[derive(Clone)] struct P { /// cache\n #[schemars(skip)] a: u32 }").is_ok()
        );
        assert!(
            err("#[derive(Clone)] struct P { #[schemars(skip)] #[input(\"a\")] a: u32 }")
                .contains("hidden")
        );
    }

    #[test]
    fn must_precede_derive() {
        assert!(err("struct P { #[input(\"a\")] a: u32 }").contains("above"));
    }
}
