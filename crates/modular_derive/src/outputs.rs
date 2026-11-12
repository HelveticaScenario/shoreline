use proc_macro::TokenStream;
use proc_macro2::{Ident, Span, TokenStream as TokenStream2};
use quote::{format_ident, quote};
use syn::ext::IdentExt;
use syn::spanned::Spanned;
use syn::{Data, DeriveInput, Fields, LitStr, Token, Type};

use crate::utils::unwrap_attr;

include!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../reserved_member_names.rs"
));

/// Why `name` cannot name a module, if it cannot. A module the DSL exposes
/// starts with `$` and an internal one with `_`. The first segment after the
/// `$` is a property of outputs and collections, so it cannot be reserved.
/// `$cycle` is the one module a reserved name belongs to: a pattern's
/// `.cycle(...)` calls it.
pub(crate) fn module_name_error(name: &str) -> Option<String> {
    if name.starts_with('_') || name == "$cycle" {
        return None;
    }
    let Some(path) = name.strip_prefix('$') else {
        return Some(format!(
            "Module name '{name}' must start with `$`, or with `_` for an internal module"
        ));
    };
    let head = path.split('.').next().unwrap_or(path);
    RESERVED_MEMBER_NAMES.contains(&head).then(|| {
        format!(
            "Module name '{name}' is reserved: `.{head}` is a built-in member of outputs, collections or patterns. Reserved names are: {RESERVED_MEMBER_NAMES:?}"
        )
    })
}

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

/// The precision of an output's values, from `precision = f64 | inherit`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ValuePrecision {
    F32,
    F64,
    Inherit,
}

impl ValuePrecision {
    fn parse(ident: &Ident) -> syn::Result<Self> {
        Ok(match ident.to_string().as_str() {
            "f64" => Self::F64,
            "inherit" => Self::Inherit,
            other => {
                return Err(syn::Error::new(
                    ident.span(),
                    format!("unknown precision `{other}`; expected f64 or inherit"),
                ));
            }
        })
    }

    /// The matching `crate::types::OutputPrecision` variant.
    fn variant(self) -> TokenStream2 {
        match self {
            Self::F32 => quote! { crate::types::OutputPrecision::F32 },
            Self::F64 => quote! { crate::types::OutputPrecision::F64 },
            Self::Inherit => quote! { crate::types::OutputPrecision::Inherit },
        }
    }

    /// Whether the port holds f64 values, given whether the module's
    /// `Inherit` outputs are f64.
    fn is_f64(self, inherit: &TokenStream2) -> TokenStream2 {
        match self {
            Self::F32 => quote! { false },
            Self::F64 => quote! { true },
            Self::Inherit => quote! { #inherit },
        }
    }
}

/// How an output's bounds arise: `bounds = params | input`, or `Fixed` from
/// its `range`.
#[derive(Clone, Copy, PartialEq, Debug)]
enum BoundsDecl {
    Unbounded,
    Fixed(f64, f64),
    Params,
    Input,
}

impl BoundsDecl {
    fn parse(ident: &Ident) -> syn::Result<Self> {
        Ok(match ident.to_string().as_str() {
            "params" => Self::Params,
            "input" => Self::Input,
            other => {
                return Err(syn::Error::new(
                    ident.span(),
                    format!("unknown bounds `{other}`; expected params or input"),
                ));
            }
        })
    }

    /// Whether the module writes these bounds beside its values.
    fn is_computed(self) -> bool {
        matches!(self, Self::Params | Self::Input)
    }

    /// The matching `crate::types::BoundsKind` variant.
    fn variant(self) -> TokenStream2 {
        match self {
            Self::Unbounded => quote! { crate::types::BoundsKind::Unbounded },
            Self::Fixed(..) => quote! { crate::types::BoundsKind::Fixed },
            Self::Params => quote! { crate::types::BoundsKind::Params },
            Self::Input => quote! { crate::types::BoundsKind::Input },
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
    value_precision: ValuePrecision,
    bounds: BoundsDecl,
}

/// Precision type for output fields
#[derive(Clone, Copy, PartialEq, Eq)]
enum OutputPrecision {
    F32,
    PolySignal,
}

/// Parsed output field data
struct OutputField {
    field_name: Ident,
    output_name: LitStr,
    precision: OutputPrecision,
    description: TokenStream2,
    output_type: OutputType,
    is_default: bool,
    range: Option<(f64, f64)>,
    /// Fixed channel count of a `PolyOutput`, independent of the module's.
    channels: Option<u32>,
    value_precision: ValuePrecision,
    bounds: BoundsDecl,
}

/// Parse output attribute tokens into OutputAttr
/// Supports:
/// - #[output("name", "description", type = cv)]
/// - #[output("name", "description", type = gate, default)]
/// - #[output("name", "description", type = cv, range = (-1.0, 1.0))]
/// - #[output("name", "description", type = cv, channels = 2)]
/// - #[output("name", "description", type = cv, precision = f64)]
/// - #[output("name", "description", type = cv, bounds = input)]
///
/// `type` is required. Gate and trig outputs emit 0–5 V, so they take no
/// `range` and are bounded by it. An output with a `range` is bounded by it;
/// `bounds` declares an output the module bounds itself.
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
        value_precision: ValuePrecision,
        bounds: BoundsDecl,
    }

    impl Parse for OutputAttrParser {
        fn parse(input: ParseStream) -> SynResult<Self> {
            // Parse first string literal (name)
            let name: LitStr = input.parse()?;

            input.parse::<Token![,]>()?;

            // Parse description string
            let description: LitStr = input.parse()?;

            let mut output_type: Option<OutputType> = None;
            let mut is_default = false;
            let mut range: Option<(f64, f64)> = None;
            let mut channels: Option<u32> = None;
            let mut value_precision = ValuePrecision::F32;
            let mut bounds: Option<(BoundsDecl, Span)> = None;
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
                    } else if ident == "precision" {
                        input.parse::<Token![=]>()?;
                        value_precision = ValuePrecision::parse(&Ident::parse_any(input)?)?;
                    } else if ident == "bounds" {
                        input.parse::<Token![=]>()?;
                        let kind = Ident::parse_any(input)?;
                        bounds = Some((BoundsDecl::parse(&kind)?, kind.span()));
                    } else {
                        return Err(syn::Error::new(
                            ident.span(),
                            format!(
                                "Unknown output attribute '{}'. Expected 'type', 'default', 'range', 'channels', 'precision' or 'bounds'",
                                ident
                            ),
                        ));
                    }
                }
            }

            let Some(output_type) = output_type else {
                return Err(syn::Error::new(
                    name.span(),
                    format!(
                        "output '{}' needs a `type` (cv, gate or trig)",
                        name.value()
                    ),
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
            let bounds = match (bounds, range) {
                (Some((_, span)), Some(_)) => {
                    return Err(syn::Error::new(
                        span,
                        "an output with a range is bounded by it; it takes no `bounds`",
                    ));
                }
                (Some((kind, _)), None) => kind,
                (None, Some((min, max))) => BoundsDecl::Fixed(min, max),
                (None, None) => BoundsDecl::Unbounded,
            };

            Ok(OutputAttrParser {
                name,
                description: Some(description),
                output_type,
                is_default,
                range,
                channels,
                value_precision,
                bounds,
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
        value_precision: parsed.value_precision,
        bounds: parsed.bounds,
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

                    // Detect field precision (f32 or PolySignal)
                    let precision = match &f.ty {
                        Type::Path(tp) => {
                            let type_name =
                                tp.path.segments.last().map(|seg| seg.ident.to_string());
                            match type_name.as_deref() {
                                Some("f32") => OutputPrecision::F32,
                                Some("PolyOutput") => OutputPrecision::PolySignal,
                                _ => {
                                    return syn::Error::new(
                                        f.ty.span(),
                                        "Output fields must have type f32 or PolyOutput",
                                    )
                                    .to_compile_error()
                                    .into();
                                }
                            }
                        }
                        _ => {
                            return syn::Error::new(
                                f.ty.span(),
                                "Output fields must have type f32, or PolyOutput",
                            )
                            .to_compile_error()
                            .into();
                        }
                    };

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

                    if output_attr.channels.is_some() && precision != OutputPrecision::PolySignal {
                        return syn::Error::new(
                            f.span(),
                            "`channels` applies only to PolyOutput fields",
                        )
                        .to_compile_error()
                        .into();
                    }
                    if output_attr.bounds.is_computed() && precision != OutputPrecision::PolySignal
                    {
                        return syn::Error::new(
                            f.span(),
                            "`bounds` applies only to PolyOutput fields",
                        )
                        .to_compile_error()
                        .into();
                    }
                    if output_attr.value_precision != ValuePrecision::F32
                        && precision != OutputPrecision::PolySignal
                    {
                        return syn::Error::new(
                            f.span(),
                            "`precision` applies only to PolyOutput fields",
                        )
                        .to_compile_error()
                        .into();
                    }

                    out.push(OutputField {
                        field_name,
                        output_name,
                        precision,
                        description,
                        output_type: output_attr.output_type,
                        is_default: output_attr.is_default,
                        range: output_attr.range,
                        channels: output_attr.channels,
                        value_precision: output_attr.value_precision,
                        bounds: output_attr.bounds,
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

    // Bounds masks give each output one bit of a `u64`.
    if outputs.len() > 64 {
        return syn::Error::new(Span::call_site(), "a module has at most 64 outputs")
            .to_compile_error()
            .into();
    }

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
            match o.precision {
                OutputPrecision::F32 => quote! { #field_name: 0.0 },
                OutputPrecision::PolySignal => {
                    quote! { #field_name: crate::poly::PolyOutput::default() }
                }
            }
        })
        .collect();

    let schema_exprs: Vec<_> = outputs
        .iter()
        .map(|o| {
            let output_name = &o.output_name;
            let description = &o.description;
            let output_type = o.output_type.variant();
            let is_polyphonic = o.precision == OutputPrecision::PolySignal;
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
            let precision = o.value_precision.variant();
            let bounds = o.bounds.variant();
            quote! {
                crate::types::OutputSchema {
                    name: #output_name.to_string(),
                    description: #description,
                    output_type: #output_type,
                    polyphonic: #is_polyphonic,
                    default: #is_default,
                    min_value: #min_value,
                    max_value: #max_value,
                    channels: #channels,
                    precision: #precision,
                    bounds: #bounds,
                }
            }
        })
        .collect();

    let set_channels_stmts: Vec<_> = outputs
        .iter()
        .filter(|o| o.precision == OutputPrecision::PolySignal)
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

    let inherit = quote! { inherit };
    let make_f64_stmts: Vec<_> = outputs
        .iter()
        .filter(|o| o.value_precision != ValuePrecision::F32)
        .map(|o| {
            let field_name = &o.field_name;
            let is_f64 = o.value_precision.is_f64(&inherit);
            quote! {
                if #is_f64 {
                    self.#field_name.make_f64();
                }
            }
        })
        .collect();
    let make_bounded_stmts: Vec<_> = outputs
        .iter()
        .enumerate()
        .filter(|(_, o)| o.bounds.is_computed())
        .map(|(i, o)| {
            let field_name = &o.field_name;
            quote! {
                if mask & (1u64 << #i) != 0 {
                    self.#field_name.make_bounded();
                }
            }
        })
        .collect();
    let mask_param = if make_bounded_stmts.is_empty() {
        quote! { _mask }
    } else {
        quote! { mask }
    };
    let uses_inherit = outputs
        .iter()
        .any(|o| o.value_precision == ValuePrecision::Inherit);
    let inherit_param = if uses_inherit {
        quote! { inherit }
    } else {
        quote! { _inherit }
    };

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

            fn make_f64_outputs(&mut self, #inherit_param: bool) {
                #(#make_f64_stmts)*
            }

            fn make_bounded(&mut self, #mask_param: u64) {
                #(#make_bounded_stmts)*
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

    // Each port is sized to its true width: f32 outputs are mono (1 channel,
    // broadcast to consumers via cycling-on-read), PolyOutput ports get their
    // fixed `channels` or else the module's derived channel count.
    let block_new_inits: Vec<_> = outputs
        .iter()
        .map(|o| {
            let field_name = &o.field_name;
            let port = match o.precision {
                OutputPrecision::F32 => {
                    quote! { crate::block_port::BlockPort::new(block_size, 1) }
                }
                OutputPrecision::PolySignal => {
                    let width = match o.channels {
                        Some(n) => {
                            let n = n as usize;
                            quote! { #n }
                        }
                        None => quote! { channel_count },
                    };
                    match o.value_precision {
                        ValuePrecision::F32 => quote! {
                            crate::block_port::BlockPort::new(block_size, #width)
                        },
                        precision => {
                            let is_f64 = precision.is_f64(&inherit);
                            quote! {
                                if #is_f64 {
                                    crate::block_port::BlockPort::new_f64(block_size, #width)
                                } else {
                                    crate::block_port::BlockPort::new(block_size, #width)
                                }
                            }
                        }
                    }
                }
            };
            quote! { #field_name: #port }
        })
        .collect();
    let block_make_bounded_stmts: Vec<_> = outputs
        .iter()
        .enumerate()
        .filter(|(_, o)| o.bounds.is_computed())
        .map(|(i, o)| {
            let field_name = &o.field_name;
            quote! {
                if mask & (1u64 << #i) != 0 {
                    self.#field_name.make_bounded();
                }
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
            if o.value_precision == ValuePrecision::F32 {
                quote! { #i => self.#field_name.get_f32_port(index, ch), }
            } else {
                quote! { #i => self.#field_name.get(index, ch), }
            }
        })
        .collect();

    let get_at_f64_arms: Vec<_> = outputs
        .iter()
        .enumerate()
        .map(|(i, o)| {
            let field_name = &o.field_name;
            quote! { #i => self.#field_name.get_f64(index, ch), }
        })
        .collect();

    let same_layout_terms: Vec<_> = outputs
        .iter()
        .filter(|o| o.value_precision == ValuePrecision::Inherit || o.bounds.is_computed())
        .map(|o| {
            let field_name = &o.field_name;
            quote! {
                && self.#field_name.lanes() == other.#field_name.lanes()
                && self.#field_name.has_bounds() == other.#field_name.has_bounds()
            }
        })
        .collect();

    let get_bounds_at_arms: Vec<_> = outputs
        .iter()
        .enumerate()
        .map(|(i, o)| {
            let field_name = &o.field_name;
            match o.bounds {
                BoundsDecl::Unbounded => quote! { #i => crate::bounds::Bounds::UNBOUNDED, },
                BoundsDecl::Fixed(min, max) => {
                    let (min, max) = (min as f32, max as f32);
                    quote! { #i => crate::bounds::Bounds::new(#min, #max), }
                }
                BoundsDecl::Params | BoundsDecl::Input => {
                    quote! { #i => self.#field_name.bounds_at(index, ch), }
                }
            }
        })
        .collect();

    let port_bounds_arms: Vec<_> = outputs
        .iter()
        .enumerate()
        .map(|(i, o)| {
            let field_name = &o.field_name;
            match o.bounds {
                BoundsDecl::Unbounded => {
                    quote! { #i => crate::bounds::PortBounds::Unbounded, }
                }
                BoundsDecl::Fixed(min, max) => {
                    let (min, max) = (min as f32, max as f32);
                    quote! { #i => crate::bounds::PortBounds::Fixed(crate::bounds::Bounds::new(#min, #max)), }
                }
                BoundsDecl::Params | BoundsDecl::Input => quote! {
                    #i => match self.#field_name.bounds_store() {
                        Some(_) if self.#field_name.channels() == 0 => {
                            crate::bounds::PortBounds::Unbounded
                        }
                        Some(store) if live => crate::bounds::PortBounds::Live { store, processed },
                        Some(store) => crate::bounds::PortBounds::Stored { store, processed },
                        None => crate::bounds::PortBounds::Unbounded,
                    }
                },
            }
        })
        .collect();

    let bounded_mask_terms: Vec<_> = outputs
        .iter()
        .enumerate()
        .map(|(i, o)| match o.bounds {
            BoundsDecl::Unbounded => quote! {},
            BoundsDecl::Fixed(..) | BoundsDecl::Params => quote! { | (1u64 << #i) },
            BoundsDecl::Input => quote! { | if inputs_bounded { 1u64 << #i } else { 0 } },
        })
        .collect();
    let inputs_bounded_param = if outputs.iter().any(|o| o.bounds == BoundsDecl::Input) {
        quote! { inputs_bounded }
    } else {
        quote! { _inputs_bounded }
    };

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
            quote! {
                #i => Some((
                    self.#field_name.as_ptr(),
                    self.#field_name.channels(),
                    self.#field_name.lanes(),
                )),
            }
        })
        .collect();

    let copy_inner_stmts: Vec<_> = outputs
        .iter()
        .map(|o| {
            let field_name = &o.field_name;
            match o.precision {
                OutputPrecision::F32 => quote! {
                    // f32 output is mono: write the single value to the port's
                    // one channel. Consumers on any channel read it back via
                    // BlockPort::get's cycling (`ch % 1 == 0`), preserving the
                    // mono-broadcast semantics downstream cables rely on.
                    self.#field_name.set_f32_port(slot, 0, inner.#field_name);
                },
                OutputPrecision::PolySignal if o.value_precision != ValuePrecision::F32 => quote! {
                    {
                        let poly = &inner.#field_name;
                        if self.#field_name.lanes() == 2 {
                            for ch in 0..self.#field_name.channels() {
                                self.#field_name.set_f64(slot, ch, poly.get_cycling_f64(ch));
                            }
                        } else {
                            for ch in 0..self.#field_name.channels() {
                                self.#field_name.set_f32_port(slot, ch, poly.get_cycling(ch));
                            }
                        }
                    }
                },
                OutputPrecision::PolySignal => quote! {
                    {
                        // Write each of this port's channels. `get_cycling`
                        // fills the port even when the producer is narrower
                        // than the port width; reads at higher consumer
                        // channels wrap via BlockPort::get's modulo.
                        let poly = &inner.#field_name;
                        for ch in 0..self.#field_name.channels() {
                            self.#field_name.set_f32_port(slot, ch, poly.get_cycling(ch));
                        }
                    }
                },
            }
        })
        .collect();

    let copy_bounds_stmts: Vec<_> = outputs
        .iter()
        .filter(|o| o.bounds.is_computed())
        .map(|o| {
            let field_name = &o.field_name;
            quote! {
                if self.#field_name.has_bounds() {
                    self.#field_name.set_bounds_row(slot, inner.#field_name.bounds());
                }
            }
        })
        .collect();

    let bounds_written_terms: Vec<_> = outputs
        .iter()
        .filter(|o| o.bounds.is_computed())
        .map(|o| {
            let field_name = &o.field_name;
            quote! { && self.#field_name.bounds_written(end) }
        })
        .collect();

    let input_bounded_terms: Vec<_> = outputs
        .iter()
        .enumerate()
        .filter(|(_, o)| o.bounds == BoundsDecl::Input)
        .map(|(i, _)| quote! { | (1u64 << #i) })
        .collect();

    let start_bounds_stmts: Vec<_> = outputs
        .iter()
        .filter(|o| o.bounds.is_computed())
        .map(|o| {
            let field_name = &o.field_name;
            quote! { self.#field_name.start_bounds_block(); }
        })
        .collect();

    let block_generated = quote! {
        /// Generated block-output buffer for #name.
        /// One `BlockPort` per output port; indexed `data[sample_index][channel]`.
        pub struct #block_outputs_name {
            #(#block_fields,)*
        }

        impl #block_outputs_name {
            /// Whether any port has `precision = inherit`.
            pub const INHERITS: bool = #uses_inherit;

            /// The `bounds = input` ports, as bits by port index.
            pub const INPUT_BOUNDED_PORTS: u64 = 0u64 #(#input_bounded_terms)*;

            /// Allocate all ports for the given block size and channel count.
            /// Call only on the main thread. f32 ports are mono; PolyOutput
            /// ports are sized to `channel_count`. `inherit` makes the
            /// `precision = inherit` ports f64.
            pub fn new(block_size: usize, channel_count: usize, #inherit_param: bool) -> Self {
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

            /// Like [`get_at`](Self::get_at), at full precision from an f64 port.
            #[inline]
            pub fn get_at_f64(&self, port_idx: usize, ch: usize, index: usize) -> f64 {
                match port_idx {
                    #(#get_at_f64_arms)*
                    _ => 0.0,
                }
            }

            /// Whether every port has the same precision as `other`'s, so
            /// two buffers of the same channel count can be swapped.
            pub fn same_layout(&self, other: &Self) -> bool {
                true #(#same_layout_terms)*
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

            /// A port's data pointer, channel width and lanes per channel, by
            /// index from [`port_index`]; `None` for an out-of-range index.
            #[inline]
            pub fn data_at(&self, port_idx: usize) -> Option<(*const f32, usize, usize)> {
                match port_idx {
                    #(#data_at_arms)*
                    _ => None,
                }
            }

            /// Copy the inner module's scalar outputs, and the bounds of its
            /// bounded ones, into this block buffer at `slot`.
            pub fn copy_from_inner(&mut self, inner: &#name, slot: usize) {
                #(#copy_inner_stmts)*
                #(#copy_bounds_stmts)*
            }

            /// Whether every bounded port has its bounds written below `end`
            /// (see [`BlockPort::bounds_written`](crate::block_port::BlockPort::bounds_written)).
            pub fn bounds_written(&self, end: usize) -> bool {
                true #(#bounds_written_terms)*
            }

            /// Begin a block on every bounded port (see
            /// [`BlockPort::start_bounds_block`](crate::block_port::BlockPort::start_bounds_block)).
            pub fn start_bounds_block(&mut self) {
                #(#start_bounds_stmts)*
            }

            /// Give the bounded ports in `mask` (bits by port index) their
            /// bounds storage. Call only on the main thread.
            pub fn make_bounded(&mut self, #mask_param: u64) {
                #(#block_make_bounded_stmts)*
            }

            /// The bounds of a port at `(index, ch)`, by index from
            /// [`port_index`](Self::port_index).
            #[inline]
            pub fn get_bounds_at(&self, port_idx: usize, ch: usize, index: usize) -> crate::bounds::Bounds {
                match port_idx {
                    #(#get_bounds_at_arms)*
                    _ => crate::bounds::Bounds::UNBOUNDED,
                }
            }

            /// How a cable reads a port's bounds: in place below `processed`,
            /// or through the wrapper when `live` (a Sample-mode module).
            pub fn port_bounds(
                &self,
                port_idx: usize,
                processed: *const std::cell::Cell<usize>,
                live: bool,
            ) -> crate::bounds::PortBounds {
                match port_idx {
                    #(#port_bounds_arms)*
                    _ => crate::bounds::PortBounds::Unbounded,
                }
            }

            /// The ports bounded in a patch where the module's tagged inputs
            /// are bounded or not, as bits by port index.
            pub fn bounded_mask(#inputs_bounded_param: bool) -> u64 {
                0u64 #(#bounded_mask_terms)*
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

    #[test]
    fn reads_how_an_output_is_bounded() {
        assert_eq!(
            parse(r#""out", "x", type = cv"#).unwrap().bounds,
            BoundsDecl::Unbounded
        );
        assert_eq!(
            parse(r#""out", "x", type = cv, range = (-1.0, 1.0)"#)
                .unwrap()
                .bounds,
            BoundsDecl::Fixed(-1.0, 1.0)
        );
        assert_eq!(
            parse(r#""gate", "x", type = gate"#).unwrap().bounds,
            BoundsDecl::Fixed(0.0, 5.0)
        );
        assert_eq!(
            parse(r#""out", "x", type = cv, bounds = input"#)
                .unwrap()
                .bounds,
            BoundsDecl::Input
        );
        assert_eq!(
            parse(r#""out", "x", type = cv, bounds = params"#)
                .unwrap()
                .bounds,
            BoundsDecl::Params
        );
        assert!(err(r#""out", "x", type = cv, bounds = sorted"#).contains("unknown bounds"));
        assert!(
            err(r#""out", "x", type = cv, range = (0.0, 1.0), bounds = input"#)
                .contains("takes no `bounds`")
        );
    }
}
