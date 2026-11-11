use convert_case::{Case, Casing};
use proc_macro::TokenStream;
use proc_macro2::{Ident, Span, TokenStream as TokenStream2};
use quote::{format_ident, quote};
use syn::ext::IdentExt;
use syn::spanned::Spanned;
use syn::{Data, DeriveInput, Fields, LitStr, Token, Type};

use crate::utils::unwrap_attr;

include!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../reserved_output_names.rs"
));

/// What an output emits: a continuous voltage, or a gate or trigger on the
/// 0–5 V convention.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum OutputType {
    Cv,
    Gate,
    Trig,
}

impl OutputType {
    fn parse(ident: &Ident) -> syn::Result<Self> {
        Ok(match ident.to_string().as_str() {
            "cv" => Self::Cv,
            "gate" => Self::Gate,
            "trig" => Self::Trig,
            other => {
                return Err(syn::Error::new(
                    ident.span(),
                    format!("unknown output type `{other}`; expected cv, gate or trig"),
                ));
            }
        })
    }

    /// The matching `crate::types::OutputType` variant.
    fn variant(self) -> TokenStream2 {
        match self {
            Self::Cv => quote! { crate::types::OutputType::Cv },
            Self::Gate => quote! { crate::types::OutputType::Gate },
            Self::Trig => quote! { crate::types::OutputType::Trig },
        }
    }
}

/// Parsed output attribute data
struct OutputAttr {
    name: LitStr,
    description: Option<LitStr>,
    output_type: OutputType,
    is_default: bool,
    range: Option<(f64, f64)>,
    channels: Option<u32>,
}

/// Parsed output field data
struct OutputField {
    field_name: Ident,
    output_name: LitStr,
    description: TokenStream2,
    output_type: OutputType,
    is_default: bool,
    range: Option<(f64, f64)>,
    /// Fixed channel count of a `PolyOutput`, independent of the module's.
    channels: Option<u32>,
}

/// Parse output attribute tokens into OutputAttr
/// Supports:
/// - #[output("name", "description", type = cv)]
/// - #[output("name", "description", type = gate, default)]
/// - #[output("name", "description", type = cv, range = (-1.0, 1.0))]
/// - #[output("name", "description", type = cv, channels = 2)]
///
/// `type` is required. Gate and trig outputs emit 0–5 V, so they take no
/// `range`.
fn parse_output_attr(tokens: TokenStream2) -> syn::Result<OutputAttr> {
    use syn::Result as SynResult;
    use syn::parse::{Parse, ParseStream};

    struct OutputAttrParser {
        name: LitStr,
        description: Option<LitStr>,
        output_type: OutputType,
        is_default: bool,
        range: Option<(f64, f64)>,
        channels: Option<u32>,
    }

    impl Parse for OutputAttrParser {
        fn parse(input: ParseStream) -> SynResult<Self> {
            // Parse first string literal (name)
            let name: LitStr = input.parse()?;
            let name_value = name.value();

            // Build expanded reserved names including snake_case variants
            let reserved_with_snake: Vec<String> = RESERVED_OUTPUT_NAMES
                .iter()
                .flat_map(|&name| {
                    let snake = name.to_case(Case::Snake);
                    if snake == name {
                        vec![name.to_string()]
                    } else {
                        vec![name.to_string(), snake]
                    }
                })
                .collect();

            if reserved_with_snake.iter().any(|r| r == &name_value) {
                return Err(syn::Error::new(
                    name.span(),
                    format!(
                        "Output name '{}' is reserved. Reserved names are: {:?}",
                        name_value, reserved_with_snake
                    ),
                ));
            }

            input.parse::<Token![,]>()?;

            // Parse description string
            let description: LitStr = input.parse()?;

            let mut output_type: Option<OutputType> = None;
            let mut is_default = false;
            let mut range: Option<(f64, f64)> = None;
            let mut channels: Option<u32> = None;
            while input.peek(Token![,]) {
                input.parse::<Token![,]>()?;

                if input.is_empty() {
                    break;
                }

                if input.peek(syn::Ident::peek_any) {
                    let ident = Ident::parse_any(input)?;
                    if ident == "type" {
                        input.parse::<Token![=]>()?;
                        if output_type.is_some() {
                            return Err(syn::Error::new(ident.span(), "duplicate `type`"));
                        }
                        output_type = Some(OutputType::parse(&Ident::parse_any(input)?)?);
                    } else if ident == "default" {
                        is_default = true;
                    } else if ident == "range" {
                        input.parse::<Token![=]>()?;
                        let content;
                        syn::parenthesized!(content in input);
                        let min: syn::LitFloat = content.parse()?;
                        content.parse::<Token![,]>()?;
                        let max: syn::LitFloat = content.parse()?;
                        range = Some((min.base10_parse()?, max.base10_parse()?));
                    } else if ident == "channels" {
                        input.parse::<Token![=]>()?;
                        let n: syn::LitInt = input.parse()?;
                        let value: u32 = n.base10_parse()?;
                        if value == 0 {
                            return Err(syn::Error::new(n.span(), "channels must be at least 1"));
                        }
                        channels = Some(value);
                    } else {
                        return Err(syn::Error::new(
                            ident.span(),
                            format!(
                                "Unknown output attribute '{}'. Expected 'type', 'default', 'range' or 'channels'",
                                ident
                            ),
                        ));
                    }
                }
            }

            let Some(output_type) = output_type else {
                return Err(syn::Error::new(
                    name.span(),
                    format!("output '{name_value}' needs a `type` (cv, gate or trig)"),
                ));
            };
            let range = match output_type {
                OutputType::Cv => range,
                OutputType::Gate | OutputType::Trig => {
                    if range.is_some() {
                        return Err(syn::Error::new(
                            name.span(),
                            "gate and trig outputs take no range; they emit 0–5 V",
                        ));
                    }
                    Some((0.0, 5.0))
                }
            };

            Ok(OutputAttrParser {
                name,
                description: Some(description),
                output_type,
                is_default,
                range,
                channels,
            })
        }
    }

    let parsed = syn::parse2::<OutputAttrParser>(tokens)?;

    Ok(OutputAttr {
        name: parsed.name,
        description: parsed.description,
        output_type: parsed.output_type,
        is_default: parsed.is_default,
        range: parsed.range,
        channels: parsed.channels,
    })
}

pub fn impl_outputs_macro(ast: &DeriveInput) -> TokenStream {
    let name = &ast.ident;

    let outputs: Vec<OutputField> = match ast.data {
        Data::Struct(ref data) => match data.fields {
            Fields::Named(ref fields) => {
                let mut out = Vec::new();
                for f in fields.named.iter() {
                    let field_name = f
                        .ident
                        .clone()
                        .expect("Expected named field in Outputs struct");

                    let output_attr_tokens = match unwrap_attr(&f.attrs, "output") {
                        Some(t) => t,
                        None => {
                            return syn::Error::new(
                                f.span(),
                                "Every field in an Outputs struct must be annotated with #[output(...)]",
                            )
                            .to_compile_error()
                            .into();
                        }
                    };

                    if let Some(doc) = f.attrs.iter().find(|a| a.path().is_ident("doc")) {
                        return syn::Error::new(
                            doc.span(),
                            "output docs live in `#[output(\"name\", \"…\")]`, not `///`",
                        )
                        .to_compile_error()
                        .into();
                    }

                    let is_poly_output = matches!(
                        &f.ty,
                        Type::Path(tp) if tp.qself.is_none()
                            && tp.path.segments.last().is_some_and(|seg| seg.ident == "PolyOutput")
                    );
                    if !is_poly_output {
                        return syn::Error::new(
                            f.ty.span(),
                            "Output fields must have type PolyOutput; a mono output is `PolyOutput` with `channels = 1`",
                        )
                        .to_compile_error()
                        .into();
                    }

                    let output_attr = match parse_output_attr(output_attr_tokens) {
                        Ok(v) => v,
                        Err(e) => return e.to_compile_error().into(),
                    };
                    let output_name = output_attr.name;
                    let description = output_attr
                        .description
                        .as_ref()
                        .map(|d| quote!(#d.to_string()))
                        .unwrap_or(quote!("".to_string()));

                    out.push(OutputField {
                        field_name,
                        output_name,
                        description,
                        output_type: output_attr.output_type,
                        is_default: output_attr.is_default,
                        range: output_attr.range,
                        channels: output_attr.channels,
                    });
                }
                out
            }
            Fields::Unnamed(_) | Fields::Unit => {
                return syn::Error::new(
                    Span::call_site(),
                    "Outputs can only be derived for structs with named fields",
                )
                .to_compile_error()
                .into();
            }
        },
        Data::Enum(_) | Data::Union(_) => {
            return syn::Error::new(Span::call_site(), "Outputs can only be derived for structs")
                .to_compile_error()
                .into();
        }
    };

    // Check for duplicate output names
    {
        let mut seen: std::collections::HashMap<String, &LitStr> = std::collections::HashMap::new();
        for output in &outputs {
            let name_value = output.output_name.value();
            if let Some(first) = seen.get(&name_value) {
                let mut err = syn::Error::new(
                    output.output_name.span(),
                    format!("Duplicate output name '{}'", name_value),
                );
                err.combine(syn::Error::new(
                    first.span(),
                    format!("'{}' first defined here", name_value),
                ));
                return err.to_compile_error().into();
            }
            seen.insert(name_value, &output.output_name);
        }
    }

    // Validate that exactly one output is marked as default
    let default_outputs: Vec<_> = outputs.iter().filter(|o| o.is_default).collect();
    if default_outputs.is_empty() {
        return syn::Error::new(
            Span::call_site(),
            format!(
                "Outputs struct '{}' must have exactly one output marked as `default`. \
                 Add `default` to one of the #[output(...)] attributes.",
                name
            ),
        )
        .to_compile_error()
        .into();
    }
    if default_outputs.len() > 1 {
        let names: Vec<_> = default_outputs
            .iter()
            .map(|o| o.output_name.value())
            .collect();
        return syn::Error::new(
            Span::call_site(),
            format!(
                "Outputs struct '{}' has {} outputs marked as `default` ({:?}), but only one is allowed.",
                name,
                default_outputs.len(),
                names
            ),
        )
        .to_compile_error()
        .into();
    }

    // Generate default value expressions for each field type
    let field_defaults: Vec<_> = outputs
        .iter()
        .map(|o| {
            let field_name = &o.field_name;
            quote! { #field_name: crate::poly::PolyOutput::default() }
        })
        .collect();

    let schema_exprs: Vec<_> = outputs
        .iter()
        .map(|o| {
            let output_name = &o.output_name;
            let description = &o.description;
            let output_type = o.output_type.variant();
            let is_default = o.is_default;
            let min_value = match o.range {
                Some((min, _)) => quote! { Some(#min) },
                None => quote! { None },
            };
            let max_value = match o.range {
                Some((_, max)) => quote! { Some(#max) },
                None => quote! { None },
            };
            let channels = match o.channels {
                Some(n) => quote! { Some(#n) },
                None => quote! { None },
            };
            quote! {
                crate::types::OutputSchema {
                    name: #output_name.to_string(),
                    description: #description,
                    output_type: #output_type,
                    default: #is_default,
                    min_value: #min_value,
                    max_value: #max_value,
                    channels: #channels,
                }
            }
        })
        .collect();

    let set_channels_stmts: Vec<_> = outputs
        .iter()
        .map(|o| {
            let field_name = &o.field_name;
            match o.channels {
                Some(n) => {
                    let n = n as usize;
                    quote! { self.#field_name.set_channels(#n); }
                }
                None => quote! { self.#field_name.set_channels(channels); },
            }
        })
        .collect();

    let generated = quote! {
        impl Default for #name {
            fn default() -> Self {
                Self {
                    #(#field_defaults,)*
                }
            }
        }

        impl crate::types::OutputStruct for #name {
            fn set_all_channels(&mut self, channels: usize) {
                #(#set_channels_stmts)*
            }

            fn schemas() -> Vec<crate::types::OutputSchema> {
                vec![
                    #(#schema_exprs,)*
                ]
            }
        }
    };

    // -----------------------------------------------------------------------
    // Generate {Name}BlockOutputs
    // -----------------------------------------------------------------------
    let name_str = name.to_string();
    let block_outputs_name = if name_str.ends_with("Outputs") {
        format_ident!("{}BlockOutputs", &name_str[..name_str.len() - 7])
    } else {
        format_ident!("{}BlockOutputs", name_str)
    };

    let block_fields: Vec<_> = outputs
        .iter()
        .map(|o| {
            let field_name = &o.field_name;
            quote! { pub #field_name: crate::block_port::BlockPort }
        })
        .collect();

    // Each port gets its fixed `channels` or else the module's derived
    // channel count.
    let block_new_inits: Vec<_> = outputs
        .iter()
        .map(|o| {
            let field_name = &o.field_name;
            match o.channels {
                Some(n) => {
                    let n = n as usize;
                    quote! { #field_name: crate::block_port::BlockPort::new(block_size, #n) }
                }
                None => quote! {
                    #field_name: crate::block_port::BlockPort::new(block_size, channel_count)
                },
            }
        })
        .collect();

    // String → index map (cold path: called once at connect time).
    let port_index_arms: Vec<_> = outputs
        .iter()
        .enumerate()
        .map(|(i, o)| {
            let output_name = &o.output_name;
            quote! { #output_name => Some(#i), }
        })
        .collect();

    // Index → field dispatch (hot path: called per-sample on audio thread).
    // `match` on `usize` lowers to a jump table or dense branch tree, much
    // tighter than per-string compare on `&str`.
    let get_at_arms: Vec<_> = outputs
        .iter()
        .enumerate()
        .map(|(i, o)| {
            let field_name = &o.field_name;
            quote! { #i => self.#field_name.get(index, ch), }
        })
        .collect();

    let channels_at_arms: Vec<_> = outputs
        .iter()
        .enumerate()
        .map(|(i, o)| {
            let field_name = &o.field_name;
            quote! { #i => self.#field_name.channels(), }
        })
        .collect();

    let data_at_arms: Vec<_> = outputs
        .iter()
        .enumerate()
        .map(|(i, o)| {
            let field_name = &o.field_name;
            quote! { #i => Some((self.#field_name.as_ptr(), self.#field_name.channels())), }
        })
        .collect();

    let copy_inner_stmts: Vec<_> = outputs
        .iter()
        .map(|o| {
            let field_name = &o.field_name;
            quote! {
                {
                    // Write each of this port's channels. `get_cycling`
                    // fills the port even when the producer is narrower
                    // than the port width; reads at higher consumer
                    // channels wrap via BlockPort::get's modulo.
                    let poly = &inner.#field_name;
                    for ch in 0..self.#field_name.channels() {
                        self.#field_name.set(slot, ch, poly.get_cycling(ch));
                    }
                }
            }
        })
        .collect();

    let block_generated = quote! {
        /// Generated block-output buffer for #name.
        /// One `BlockPort` per output port; indexed `data[sample_index][channel]`.
        pub struct #block_outputs_name {
            #(#block_fields,)*
        }

        impl #block_outputs_name {
            /// Allocate all ports for the given block size and channel count.
            /// Call only on the main thread. Ports without a fixed
            /// `channels` are sized to `channel_count`.
            pub fn new(block_size: usize, channel_count: usize) -> Self {
                Self {
                    #(#block_new_inits,)*
                }
            }

            /// Resolve a port name to its index. Cold path — call once at
            /// connect time and cache the result on the cable.
            ///
            /// Returns `None` for unknown ports.
            #[inline]
            pub fn port_index(name: &str) -> Option<usize> {
                match name {
                    #(#port_index_arms)*
                    _ => None,
                }
            }

            /// Hot-path read by port index. `port_idx` must come from
            /// [`port_index`] (a stale or out-of-range index returns 0.0).
            ///
            /// Lowers to a `match`-on-`usize`, which the optimiser turns into
            /// a jump table or dense branch tree — typically one branch per
            /// read regardless of port count.
            ///
            /// There is intentionally **no** `&str` overload: callers must
            /// resolve the port name once via [`port_index`] and cache the
            /// result. String lookup on the audio thread is exactly the cost
            /// this design avoids.
            #[inline]
            pub fn get_at(&self, port_idx: usize, ch: usize, index: usize) -> f32 {
                match port_idx {
                    #(#get_at_arms)*
                    _ => 0.0,
                }
            }

            /// True channel width of a port, by index from [`port_index`] (a
            /// stale or out-of-range index returns 0). Reads at `ch >= width`
            /// cycle back over the port's channels, so a consumer that must
            /// not cycle checks this first.
            #[inline]
            pub fn channels_at(&self, port_idx: usize) -> usize {
                match port_idx {
                    #(#channels_at_arms)*
                    _ => 0,
                }
            }

            /// A port's data pointer and channel width, by index from
            /// [`port_index`]; `None` for an out-of-range index.
            #[inline]
            pub fn data_at(&self, port_idx: usize) -> Option<(*const f32, usize)> {
                match port_idx {
                    #(#data_at_arms)*
                    _ => None,
                }
            }

            /// Copy the inner module's scalar outputs into this block buffer at `slot`.
            pub fn copy_from_inner(&mut self, inner: &#name, slot: usize) {
                #(#copy_inner_stmts)*
            }

        }
    };

    let mut all_generated = quote!(#generated);
    all_generated.extend(block_generated);
    all_generated.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> syn::Result<OutputAttr> {
        parse_output_attr(src.parse().unwrap())
    }

    fn err(src: &str) -> String {
        match parse(src) {
            Ok(_) => panic!("expected an error for {src}"),
            Err(e) => e.to_string(),
        }
    }

    #[test]
    fn reads_the_output_type() {
        let attr = parse(r#""gate", "high while a note plays", type = gate"#).unwrap();
        assert_eq!(attr.output_type, OutputType::Gate);
        assert_eq!(attr.range, Some((0.0, 5.0)));
        let attr = parse(r#""output", "the mix", default, type = cv"#).unwrap();
        assert!(attr.output_type == OutputType::Cv && attr.is_default);
    }

    #[test]
    fn requires_a_known_output_type() {
        assert!(err(r#""output", "the mix", default"#).contains("needs a `type`"));
        assert!(err(r#""output", "the mix", type = audio"#).contains("unknown output type"));
        assert!(err(r#""output", "the mix", type = cv, type = gate"#).contains("duplicate"));
        assert!(err(r#""gate", "held", type = gate, range = (0.0, 5.0)"#).contains("no range"));
    }
}
