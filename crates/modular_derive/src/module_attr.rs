use convert_case::{Case, Casing};
use proc_macro::TokenStream;
use proc_macro2::{Ident, Span, TokenStream as TokenStream2};
use quote::{format_ident, quote};
use syn::{Data, DeriveInput, Fields, LitStr, Token, Type, punctuated::Punctuated};

use crate::utils::unwrap_attr;

/// Parsed module attribute data
struct ModuleAttr {
    name: LitStr,
    lanes: Option<u8>,
    lanes_param: Option<LitStr>,
    lanes_param_default: Option<u8>,
    /// Custom function to derive lane count from params struct.
    /// The function must have signature: fn(&ParamsStruct) -> Option<usize>
    lanes_derive: Option<syn::Path>,
    /// Path to the module's `crate::module_docs::ModuleDocs` const.
    docs: syn::Path,
}

struct ArgAttr {
    name: Ident,
}

impl syn::parse::Parse for ArgAttr {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let name: Ident = input.parse()?;
        Ok(ArgAttr { name })
    }
}

// ---------------------------------------------------------------------------
// Attribute-macro argument parser
// ---------------------------------------------------------------------------

/// All configuration parsed from `#[module(...)]` attribute arguments.
///
/// Idiomatic key=value syntax:
/// ```text
/// #[module(
///     name = "$sine",
///     docs = SINE_DOCS,
///     lanes = 2,
///     args(freq, engine),
///     stateful,
///     patch_update,
///     has_init,
///     experimental,
/// )]
/// ```
pub struct ModuleAttrArgs {
    module: ModuleAttr,
    args: Vec<ArgAttr>,
    stateful: bool,
    patch_update: bool,
    has_init: bool,
    has_prepare_resources: bool,
    clock_sync: bool,
    block: bool,
    experimental: bool,
    stereo: Option<Ident>,
}

impl syn::parse::Parse for ModuleAttrArgs {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut name: Option<LitStr> = None;
        let mut lanes: Option<u8> = None;
        let mut lanes_param: Option<LitStr> = None;
        let mut lanes_param_default: Option<u8> = None;
        let mut lanes_derive: Option<syn::Path> = None;
        let mut args: Vec<ArgAttr> = Vec::new();
        let mut docs: Option<syn::Path> = None;
        let mut stateful = false;
        let mut patch_update = false;
        let mut has_init = false;
        let mut has_prepare_resources = false;
        let mut clock_sync = false;
        let mut block = false;
        let mut experimental = false;
        let mut stereo: Option<Ident> = None;

        while !input.is_empty() {
            let ident: Ident = input.parse()?;
            match ident.to_string().as_str() {
                "name" => {
                    input.parse::<Token![=]>()?;
                    name = Some(input.parse()?);
                }
                "lanes" => {
                    input.parse::<Token![=]>()?;
                    let lit: syn::LitInt = input.parse()?;
                    let n: u8 = lit.base10_parse()?;
                    // Must match modular_core::poly::PORT_MAX_LANES
                    const MAX: u8 = 64;
                    if !(1..=MAX).contains(&n) {
                        return Err(syn::Error::new(
                            lit.span(),
                            format!("lanes must be between 1 and {MAX}, got {n}"),
                        ));
                    }
                    lanes = Some(n);
                }
                "lanes_param" => {
                    input.parse::<Token![=]>()?;
                    lanes_param = Some(input.parse()?);
                }
                "lanes_param_default" => {
                    input.parse::<Token![=]>()?;
                    let lit: syn::LitInt = input.parse()?;
                    lanes_param_default = Some(lit.base10_parse()?);
                }
                "lanes_derive" => {
                    input.parse::<Token![=]>()?;
                    lanes_derive = Some(input.parse()?);
                }
                "args" => {
                    let content;
                    syn::parenthesized!(content in input);
                    let parsed: Punctuated<ArgAttr, Token![,]> =
                        Punctuated::parse_terminated(&content)?;
                    args = parsed.into_iter().collect();
                }
                "docs" => {
                    input.parse::<Token![=]>()?;
                    docs = Some(input.parse()?);
                }
                "stateful" => {
                    stateful = true;
                }
                "patch_update" => {
                    patch_update = true;
                }
                "has_init" => {
                    has_init = true;
                }
                "has_prepare_resources" => {
                    has_prepare_resources = true;
                }
                "clock_sync" => {
                    clock_sync = true;
                }
                "block" => {
                    block = true;
                }
                "experimental" => {
                    experimental = true;
                }
                "stereo" => {
                    input.parse::<Token![=]>()?;
                    let mode: Ident = input.parse()?;
                    if mode != "mono" && mode != "stereo" && mode != "derived" {
                        return Err(syn::Error::new(
                            mode.span(),
                            "stereo must be `mono`, `stereo` or `derived`; a module lifts by default",
                        ));
                    }
                    stereo = Some(mode);
                }
                other => {
                    return Err(syn::Error::new(
                        ident.span(),
                        format!(
                            "Unknown module attribute '{other}'. Expected one of: \
                             name, docs, lanes, lanes_param, \
                             lanes_param_default, lanes_derive, args, \
                             stateful, patch_update, has_init, has_prepare_resources, clock_sync, block, \
                             experimental, stereo"
                        ),
                    ));
                }
            }

            // Consume trailing comma if present
            let _ = input.parse::<Token![,]>();
        }

        let name = name.ok_or_else(|| {
            syn::Error::new(
                proc_macro2::Span::call_site(),
                "missing `name` in #[module(...)]",
            )
        })?;
        if let Some(message) = crate::outputs::module_name_error(&name.value()) {
            return Err(syn::Error::new(name.span(), message));
        }
        let docs = docs.ok_or_else(|| {
            syn::Error::new(
                proc_macro2::Span::call_site(),
                "missing `docs = …` in #[module(...)]: name the module's `ModuleDocs` const",
            )
        })?;

        Ok(ModuleAttrArgs {
            module: ModuleAttr {
                name,
                lanes,
                lanes_param,
                lanes_param_default,
                lanes_derive,
                docs,
            },
            args,
            stateful,
            patch_update,
            has_init,
            has_prepare_resources,
            clock_sync,
            block,
            experimental,
            stereo,
        })
    }
}

/// Attribute-style proc macro for declaring audio modules.
///
/// # Syntax
///
/// ```rust,ignore
/// #[module(
///     name = "$sine",
///     docs = SINE_DOCS,                        // the module's ModuleDocs const
///     // Lane count configuration (at most one):
///     // lanes = 2,                         // hardcoded
///     // lanes_param = "lanes",           // read from param field
///     // lanes_param_default = 1,            // default when param absent
///     // lanes_derive = my_derive_fn,        // custom function
///     //
///     // Positional DSL arguments (optional):
///     // args(freq, engine),
///     //
///     // Flags (optional):
///     // stateful,      // implements StatefulModule
///     // patch_update,  // implements PatchUpdateHandler
///     // has_init,      // has fn init(&mut self, sample_rate: f32)
///     // block,         // also has fn process_block(&mut self, outputs, range,
///     //                // sample_rate: f32) -> Option<()>, run in Block mode
///     //                // (see crate::block_input)
///     // experimental,  // API and sound may still change; shown to users
///     //                // in editor docs
/// )]
/// pub struct MyModule { ... }
/// ```
///
/// The struct **must** have a field named `outputs` whose type derives `Outputs`,
/// and a field named `params` whose type derives `Deserialize`, `JsonSchema`,
/// `Connect`, and `LaneCount`.
///
/// Module structs do **not** need to derive `Default`. The proc macro generates
/// per-field initialization in the constructor: `params` comes from deserialization,
/// `_lane_count` from the computed lane count, and all other fields use
/// `Default::default()` on their individual types.
pub fn module_impl(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr_args = syn::parse_macro_input!(attr as ModuleAttrArgs);
    let mut ast: DeriveInput = syn::parse_macro_input!(item as DeriveInput);

    // Strip any leftover helper attributes that we've absorbed (safety net for migration)
    ast.attrs.retain(|a| {
        !a.path().is_ident("module")
            && !a.path().is_ident("args")
            && !a.path().is_ident("stateful")
            && !a.path().is_ident("patch_update")
            && !a.path().is_ident("has_init")
            && !a.path().is_ident("has_prepare_resources")
    });

    // Inject `_lane_count: usize` field into the struct so that
    // `self.lane_count()` can return a precomputed value set by the
    // main thread via the constructor.
    if let Data::Struct(ref mut data_struct) = ast.data
        && let Fields::Named(ref mut fields) = data_struct.fields
    {
        let field: syn::Field = syn::parse_quote! {
            pub _lane_count: usize
        };
        fields.named.push(field);
        let field: syn::Field = syn::parse_quote! {
            pub _channels: usize
        };
        fields.named.push(field);
    }

    // Inject `_block_index: Cell<usize>` so inner DSP can read the current
    // sample position within the block via `self.current_block_index()`.
    // The wrapper's `ensure_processed_to` writes the slot index here before
    // each `inner.update` call.
    if let Data::Struct(ref mut data_struct) = ast.data
        && let Fields::Named(ref mut fields) = data_struct.fields
    {
        let field: syn::Field = syn::parse_quote! {
            pub _block_index: std::cell::Cell<usize>
        };
        fields.named.push(field);
    }

    match impl_module_macro_attr(&ast, &attr_args) {
        Ok(generated) => {
            let mut output = quote!(#ast);
            output.extend(generated);
            output.into()
        }
        Err(e) => e.to_compile_error().into(),
    }
}

fn impl_module_macro_attr(
    ast: &DeriveInput,
    attr_args: &ModuleAttrArgs,
) -> syn::Result<TokenStream2> {
    let name = &ast.ident;
    let module_name = &attr_args.module.name;

    // The `docs` const is the module's only user-facing documentation.
    if let Some(doc) = ast
        .attrs
        .iter()
        .find(|a| a.path().is_ident("doc") && matches!(a.meta, syn::Meta::NameValue(_)))
    {
        return Err(syn::Error::new_spanned(
            doc,
            "module docs live in the `ModuleDocs` const named by `docs = …`, not `///`",
        ));
    }
    let docs_path = &attr_args.module.docs;

    // Store lanes info for lane_count generation
    let hardcoded_lanes = attr_args.module.lanes;
    let lanes_param_name = attr_args.module.lanes_param.clone();
    let lanes_param_default_val = attr_args.module.lanes_param_default;
    let lanes_derive_fn = &attr_args.module.lanes_derive;

    let module_lanes = match attr_args.module.lanes {
        Some(n) => quote! { Some(#n) },
        None => quote! { None },
    };
    let module_lanes_param = match &attr_args.module.lanes_param {
        Some(s) => quote! { Some(#s.to_string()) },
        None => quote! { None },
    };
    let module_lanes_param_default = match attr_args.module.lanes_param_default {
        Some(n) => quote! { Some(#n) },
        None => quote! { None },
    };

    let positional_args_exprs: Vec<TokenStream2> = attr_args
        .args
        .iter()
        .map(|arg| {
            let arg_name = arg.name.to_string();
            quote! {
                crate::types::PositionalArg {
                    name: #arg_name.to_string(),
                }
            }
        })
        .collect();

    // The module struct must contain a field named `outputs`.
    // Also collect all fields for per-field initialization in the constructor.
    // Mirror outputs.rs's BlockOutputs naming: "{Foo}Outputs" -> "{Foo}BlockOutputs".
    fn block_outputs_type_from(outputs_ty: &Type) -> syn::Result<Type> {
        match outputs_ty {
            Type::Path(tp) if tp.qself.is_none() => {
                let mut path = tp.path.clone();
                let last = path.segments.last_mut().ok_or_else(|| {
                    syn::Error::new(
                        Span::call_site(),
                        "outputs type must have at least one path segment",
                    )
                })?;
                let name = last.ident.to_string();
                let new_name = if let Some(stripped) = name.strip_suffix("Outputs") {
                    format!("{stripped}BlockOutputs")
                } else {
                    format!("{name}BlockOutputs")
                };
                last.ident = Ident::new(&new_name, last.ident.span());
                Ok(Type::Path(syn::TypePath { qself: None, path }))
            }
            _ => Err(syn::Error::new(
                Span::call_site(),
                "outputs field type must be a simple path, e.g. `SineOscOutputs`",
            )),
        }
    }

    let (outputs_ty, module_field_inits, has_state_field, has_lane_state_field): (
        Type,
        Vec<TokenStream2>,
        bool,
        bool,
    ) = match ast.data {
        Data::Struct(ref data) => match data.fields {
            Fields::Named(ref fields) => {
                // Disallow legacy per-field #[output] annotations on the module struct.
                if fields
                    .named
                    .iter()
                    .any(|f| unwrap_attr(&f.attrs, "output").is_some())
                {
                    return Err(syn::Error::new(
                        Span::call_site(),
                        "#[module] expects an `outputs` field (a struct that derives Outputs); do not annotate module fields with #[output(...)]",
                    ));
                }

                let outputs_field = fields
                    .named
                    .iter()
                    .find(|f| f.ident.as_ref().map(|i| i == "outputs").unwrap_or(false));

                let outputs_ty = match outputs_field {
                    Some(f) => f.ty.clone(),
                    None => {
                        return Err(syn::Error::new(
                            Span::call_site(),
                            "#[module] requires a field named `outputs` whose type derives Outputs",
                        ));
                    }
                };

                let has_state = fields
                    .named
                    .iter()
                    .any(|f| f.ident.as_ref().map(|i| i == "state").unwrap_or(false));
                let has_lane_state = fields
                    .named
                    .iter()
                    .any(|f| f.ident.as_ref().map(|i| i == "lane_state").unwrap_or(false));

                // Generate per-field initialization for the inner module struct.
                // - `params` → use deserialized params
                // - `_lane_count` → use deserialized lane count
                // - `outputs` and `state` → use Default::default()
                // - `lane_state` → a boxed slice of `lane_count` default
                //   elements (per-lane state, sized to the patch's lanes)
                // - other fields → error
                let field_inits: Vec<TokenStream2> = fields
                    .named
                    .iter()
                    .map(|f| {
                        let field_name = f.ident.as_ref().unwrap();
                        let field_name_str = field_name.to_string();
                        match field_name_str.as_str() {
                            "params" => Ok(quote! { params: *concrete_params }),
                            "_lane_count" => Ok(quote! { _lane_count: deserialized.lane_count }),
                            "_channels" => Ok(quote! { _channels: deserialized.channels }),
                            "_block_index" => Ok(quote! { _block_index: Default::default() }),
                            "outputs" | "state" => Ok(quote! { #field_name: Default::default() }),
                            "lane_state" => Ok(quote! {
                                lane_state: (0..deserialized.lane_count)
                                    .map(|_| ::core::default::Default::default())
                                    .collect::<::std::vec::Vec<_>>()
                                    .into_boxed_slice()
                            }),
                            other => Err(syn::Error::new(
                                field_name.span(),
                                format!(
                                    "Module struct field `{other}` is not allowed. Only \
                                     `state`, `lane_state`, `outputs`, and `params` are permitted.",
                                ),
                            )),
                        }
                    })
                    .collect::<Result<Vec<_>, _>>()?
                    .into_iter()
                    .collect();

                (outputs_ty, field_inits, has_state, has_lane_state)
            }
            Fields::Unnamed(_) | Fields::Unit => {
                return Err(syn::Error::new(
                    Span::call_site(),
                    "#[module] can only be applied to structs with named fields",
                ));
            }
        },
        Data::Enum(_) | Data::Union(_) => {
            return Err(syn::Error::new(
                Span::call_site(),
                "#[module] can only be applied to structs",
            ));
        }
    };
    let block_outputs_ty = block_outputs_type_from(&outputs_ty)?;

    let struct_name = format_ident!("{}Sampleable", name);
    let constructor_name = format_ident!("{}Constructor", name)
        .to_string()
        .to_case(Case::Snake);
    let constructor_name = Ident::new(&constructor_name, Span::call_site());
    let params_struct_name = format_ident!("{}Params", name);

    // Extract generics for proper impl blocks
    let (impl_generics, ty_generics, where_clause) = ast.generics.split_for_impl();

    // For the wrapper struct, we need to replace all lifetime parameters with 'static
    // since Sampleable requires 'static. Build a static version of ty_generics.
    let static_ty_generics = {
        let params = ast
            .generics
            .params
            .iter()
            .map(|p| match p {
                syn::GenericParam::Lifetime(_) => quote!('static),
                syn::GenericParam::Type(t) => {
                    let ident = &t.ident;
                    quote!(#ident)
                }
                syn::GenericParam::Const(c) => {
                    let ident = &c.ident;
                    quote!(#ident)
                }
            })
            .collect::<Vec<_>>();
        if params.is_empty() {
            quote!()
        } else {
            quote!(<#(#params),*>)
        }
    };

    let is_stateful = attr_args.stateful;
    let experimental = attr_args.experimental;
    let stereo_mode = match attr_args.stereo.as_ref().map(|i| i.to_string()).as_deref() {
        Some("mono") => quote! { crate::types::StereoMode::Mono },
        Some("derived") => quote! { crate::types::StereoMode::Derived },
        Some(_) => quote! { crate::types::StereoMode::Stereo },
        None => quote! { crate::types::StereoMode::Lift },
    };

    // Editor-state publishing. Only stateful modules opt in; they write their
    // live state into a pre-allocated, type-erased slot on the audio thread
    // without allocating. The immutable metadata is built and merged on the main
    // thread (see crate::module_state). Non-stateful modules produce nothing.
    let write_module_state_impl = if is_stateful {
        quote! {
            fn write_module_state(&self, out: &mut dyn crate::module_state::ModuleLiveState) {
                use crate::types::StatefulModule;
                // SAFETY: Audio thread has exclusive access. See crate::types module documentation.
                let module = unsafe { &*self.module.get() };
                module.write_module_state(out);
            }
        }
    } else {
        // Non-stateful modules use the Sampleable default (no-op).
        quote! {}
    };

    // Check for has_init flag
    let has_init_call = if attr_args.has_init {
        quote! {
            // SAFETY: We just created sampleable, no one else has access yet.
            unsafe { (*sampleable.module.get()).init(sample_rate); }
        }
    } else {
        quote! {}
    };

    // Check for has_prepare_resources flag
    let prepare_resources_impl = if attr_args.has_prepare_resources {
        quote! {
            fn prepare_resources(
                &self,
                wav_data: &std::collections::HashMap<String, std::sync::Arc<crate::types::WavData>>,
            ) {
                // SAFETY: Called on the main thread between construction and
                // queueing the module for the audio thread. No other
                // references to `self.module` exist at this point.
                let module = unsafe { &mut *self.module.get() };
                module.prepare_resources_impl(wav_data, self.sample_rate);
            }
        }
    } else {
        quote! {}
    };

    // Check for patch_update flag
    let on_patch_update_impl = if attr_args.patch_update {
        quote! {
            fn on_patch_update(&self) {
                use crate::types::PatchUpdateHandler;
                // SAFETY: Audio thread has exclusive access. See crate::types module documentation.
                let module = unsafe { &mut *self.module.get() };
                PatchUpdateHandler::on_patch_update(module);
            }
        }
    } else {
        quote! {
            fn on_patch_update(&self) {}
        }
    };

    // Check for clock_sync flag
    let clock_sync_impl = if attr_args.clock_sync {
        quote! {
            fn sync_external_clock(&self, state: crate::types::ExternalClockState) {
                let module = unsafe { &mut *self.module.get() };
                module.sync_external_clock_impl(state);
            }

            fn clear_external_sync(&self) {
                let module = unsafe { &mut *self.module.get() };
                module.clear_external_sync();
            }

            fn reset_loop_index(&self) {
                let module = unsafe { &mut *self.module.get() };
                module.reset_loop_index_impl();
            }
        }
    } else {
        quote! {}
    };

    // Generate transfer_state_from body.
    //
    // `state` (module-level, non-per-lane) is always carried over with a
    // swap — it is the same size in both modules.
    let state_transfer = if has_state_field {
        quote! {
            std::mem::swap(&mut new_inner.state, &mut old_inner.state);
        }
    } else {
        quote! {}
    };
    // `lane_state` (per-lane) carries over the overlapping lanes:
    // equal lengths swap the whole boxed slice (O(1) pointer swap); otherwise
    // the first `min(old, new)` lanes are swapped element-wise and the
    // surplus keeps its freshly-initialised value. When the channels per lane
    // change, lane `l` side `c` takes old lane `l` side `c`, and a side the
    // old module lacked starts fresh. No path allocates or clones on the audio
    // thread.
    let lane_state_transfer = if has_lane_state_field {
        quote! {
            let new_len = new_inner.lane_state.len();
            let old_len = old_inner.lane_state.len();
            let new_channels = new_inner._channels.max(1);
            let old_channels = old_inner._channels.max(1);
            if new_channels != old_channels {
                for k in 0..new_len {
                    let side = k % new_channels;
                    let j = (k / new_channels) * old_channels + side;
                    if side < old_channels && j < old_len {
                        std::mem::swap(&mut new_inner.lane_state[k], &mut old_inner.lane_state[j]);
                    }
                }
            } else if new_len == old_len {
                std::mem::swap(&mut new_inner.lane_state, &mut old_inner.lane_state);
            } else {
                let n = new_len.min(old_len);
                new_inner.lane_state[..n].swap_with_slice(&mut old_inner.lane_state[..n]);
            }
        }
    } else {
        quote! {}
    };
    let transfer_state_body = quote! {
        #state_transfer
        #lane_state_transfer
    };

    // Generate the lane count derivation function name
    let lane_count_fn_name = format_ident!(
        "__{}_derive_lane_count",
        name.to_string().to_case(Case::Snake)
    );

    // Generate the core lane count implementation that works with typed params.
    let lane_count_fn_impl = if let Some(custom_fn) = lanes_derive_fn {
        quote! {
            #[inline]
            fn #lane_count_fn_name(params: &#params_struct_name) -> usize {
                #custom_fn(params)
            }
        }
    } else {
        match (hardcoded_lanes, &lanes_param_name, lanes_param_default_val) {
            (Some(n), _, _) => {
                let n = n as usize;
                quote! {
                    #[inline]
                    fn #lane_count_fn_name(_params: &#params_struct_name) -> usize {
                        #n
                    }
                }
            }
            (None, Some(param_name), default_val) => {
                let param_ident = Ident::new(&param_name.value(), param_name.span());
                match default_val {
                    Some(default) => {
                        let default = default as usize;
                        quote! {
                            #[inline]
                            fn #lane_count_fn_name(params: &#params_struct_name) -> usize {
                                let param_value = params.#param_ident;
                                if param_value > 0 {
                                    param_value.clamp(1, crate::poly::PORT_MAX_LANES)
                                } else {
                                    #default
                                }
                            }
                        }
                    }
                    None => {
                        quote! {
                            #[inline]
                            fn #lane_count_fn_name(params: &#params_struct_name) -> usize {
                                params.#param_ident.clamp(1, crate::poly::PORT_MAX_LANES)
                            }
                        }
                    }
                }
            }
            (None, None, _) => {
                quote! {
                    #[inline]
                    fn #lane_count_fn_name(params: &#params_struct_name) -> usize {
                        use crate::types::PolySignalFields;
                        let fields = params.poly_signal_fields();
                        let refs: Vec<&crate::poly::PolySignal> = fields.into_iter().collect();
                        crate::poly::PolySignal::max_lanes(&refs).max(1) as usize
                    }
                }
            }
        }
    };

    // Runs `update` for every slot up to `target`.
    let sample_loop = quote! {
        while self.index.get() < target {
            let i = self.index.get();
            unsafe {
                let module = &mut *self.module.get();
                // Expose the current slot index to the inner DSP
                // via `self.current_block_index()` before running
                // the per-sample update. Used by modules that
                // need slot-aware reads/writes against shared
                // resources (BufferWrite / DelayRead's circular
                // buffer cursor, etc).
                module._block_index.set(i);
                module.update(self.sample_rate);
                (*self.block_outputs.get()).copy_from_inner(&module.outputs, i);
            }
            self.index.set(i + 1);
        }
    };
    // The sample loop appears once, so `update` and the output copy have a
    // single call site to inline into.
    let process_loop = if attr_args.block {
        quote! {
            let block = self.mode == crate::types::ProcessingMode::Block && !self.f64_outputs;
            // SAFETY: Audio thread has exclusive access. See crate::types
            // module documentation. Each call's borrows end with the call.
            let process = |range: std::ops::Range<usize>| unsafe {
                (*self.module.get()).process_block(
                    &mut *self.block_outputs.get(),
                    range,
                    self.sample_rate,
                )
            };
            let mut start = start_index;
            if block && process(start_index..target).is_some() {
                start = target;
            }
            // Block mode retries in sub-ranges short enough for every input
            // to resolve (see crate::block_input), halving the length while a
            // sub-range is declined; a single slot that is still declined runs
            // `update`. Feedback cycles (Sample mode) run `update` one slot at
            // a time, so each read sees the cycle's previous slot.
            let mut step = if block {
                let lanes = unsafe { (*self.module.get())._lane_count.max(1) };
                (crate::block_input::SCRATCH_LEN / lanes).max(1)
            } else {
                0
            };
            while start < target {
                let mut end = target;
                if block {
                    end = (start + step).min(target);
                    self.index.set(start);
                    if process(start..end).is_some() {
                        start = end;
                        continue;
                    }
                    if end - start > 1 {
                        step = (end - start) / 2;
                        continue;
                    }
                    self.index.set(start);
                }
                let target = end;
                #sample_loop
                start = end;
            }
            self.index.set(target);
        }
    } else {
        sample_loop
    };

    let generated = quote! {
        // Generated core lane count function (used by derive_lane_count and initial default)
        // IMPORTANT: This function should never be called within the audio thread.
        // It may be computationally expensive. It should only be called in non-audio-thread contexts.
        #lane_count_fn_impl

        impl #impl_generics #name #ty_generics #where_clause {
            /// Returns the precomputed lane count set during construction.
            #[inline]
            pub fn lane_count(&self) -> usize {
                self._lane_count
            }

            /// Channels per lane, set during construction: 1, or 2 when the
            /// module is stereo. Lane `l` side `c` is lane `l * channels + c`
            /// of [`lane_count`](Self::lane_count).
            #[inline]
            pub fn channels(&self) -> usize {
                self._channels
            }

            /// Returns the current sample slot index within the block being
            /// processed. Only valid to call from inside `update()` — the
            /// wrapper writes the slot index here before each per-sample
            /// inner update.
            #[inline]
            pub fn current_block_index(&self) -> usize {
                self._block_index.get()
            }
        }

        /// Generated wrapper struct for audio-thread-only module access.
        ///
        /// # Safety Model (UnsafeCell)
        ///
        /// This struct uses `UnsafeCell` instead of `Mutex`/`RwLock` for interior mutability.
        /// This is safe because:
        ///
        /// 1. **Exclusive Audio Thread Ownership**: After construction, all modules live in
        ///    `AudioProcessor::patch` which is owned exclusively by the audio thread closure.
        ///    See `crates/modular/src/audio.rs` `make_stream()`.
        ///
        /// 2. **Command Queue Isolation**: The main thread communicates via `PatchUpdate`
        ///    commands through an `rtrb` SPSC queue. It never directly accesses module state.
        ///
        /// 3. **No Escaping References**: Owned modules are stored in `Patch::sampleables` and
        ///    are never aliased across threads after being added to the patch.
        ///
        /// ## Invariants (DO NOT VIOLATE)
        ///
        /// - **NEVER** call Sampleable trait methods from the main thread
        /// - **NEVER** share module ownership across threads
        /// - **NEVER** access Patch::sampleables from outside AudioProcessor
        /// - **ALWAYS** use the command queue for main→audio communication
        ///
        /// Violating these invariants will cause undefined behavior (data races).
        struct #struct_name {
            id: String,
            module: std::cell::UnsafeCell<#name #static_ty_generics>,
            sample_rate: f32,
            /// Per-block sample-index cursor. `start_block()` resets it to 0
            /// at the top of each internal `block_size` block; `ensure_processed_to`
            /// advances it as samples get written. Embedded `Signal::Cable`s
            /// in this module's params hold a raw back-pointer to it (set
            /// during `connect()`) and read it inline when fetching upstream
            /// block samples.
            index: std::cell::Cell<usize>,
            /// Block-sized output buffer. One `BlockPort` per output port,
            /// each pre-allocated to `block_size` samples. `ensure_processed_to`
            /// writes the inner module's per-sample outputs into slot `i`
            /// after each `inner.update`, and downstream cables read from it
            /// via `get_value_at(port_idx, lane, index)`.
            ///
            /// Allocated once at construction; never resized on the audio
            /// thread.
            block_outputs: std::cell::UnsafeCell<#block_outputs_ty>,
            /// Counts the patch swaps this module's lineage has gone through;
            /// stamped into the params so a `SwapGlide` can tell a new swap
            /// from the one it already handled.
            glide_generation: std::cell::Cell<u64>,
            /// Block size pinned at construction. Mirrors the BlockPort length.
            block_size: usize,
            /// Block vs. Sample mode, assigned by graph_analysis cycle
            /// detection. Block-mode wrappers compute the entire block on
            /// first request; Sample-mode wrappers compute one sample per
            /// request (used for modules inside feedback cycles).
            mode: crate::types::ProcessingMode,
            /// Whether the `precision = inherit` outputs are f64. Block inputs
            /// resolve at f32, so such a module always runs `update`.
            f64_outputs: bool,
            /// Reentrancy guard for `get_value_at`. Set by `ensure_processed_to`
            /// while it runs the inner update loop. When a cycle reads back
            /// into a module that is mid-computation, `get_value_at` returns
            /// the slot one step *behind* the requested one (wrapping at the
            /// block boundary) so the feedback path sees exactly a 1-sample
            /// delay.
            computing: std::cell::Cell<bool>,
        }
        impl #struct_name {
            /// The slot a read of `index` sees, rendered first: `index`
            /// itself, or during a re-entrant read from a feedback cycle the
            /// slot before it.
            #[inline]
            fn readable_slot(&self, index: usize) -> usize {
                if self.computing.get() {
                    return if index == 0 {
                        self.block_size.saturating_sub(1)
                    } else {
                        index - 1
                    };
                }
                let target = match self.mode {
                    // Fill the block greedily, but never past the audio
                    // callback's render ceiling: state that advances beyond
                    // the emit boundary would be transferred "from the
                    // future" by a mid-block patch swap.
                    crate::types::ProcessingMode::Block => {
                        self.block_size.min(crate::types::block_render_ceiling())
                    }
                    // Inclusive — process up through the requested slot.
                    crate::types::ProcessingMode::Sample => index + 1,
                };
                crate::types::Sampleable::ensure_processed_to(self, target);
                index
            }
        }

        impl crate::types::Sampleable for #struct_name {
            fn start_block(&self) {
                self.index.set(0);
                // Let modules with circular-buffer outputs (e.g. BufferWrite)
                // advance their write cursor once per internal block. Inner
                // `update` then offsets the per-sample write position by
                // `current_block_index()`.
                unsafe {
                    let module = &mut *self.module.get();
                    crate::types::OutputStruct::tick_buffers(
                        &mut module.outputs,
                        self.block_size,
                    );
                    (*self.block_outputs.get()).start_bounds_block();
                }
            }

            fn set_initial_index(&self, slot: usize) {
                self.index.set(slot.min(self.block_size));
            }

            fn ensure_processed_to(&self, target: usize) {
                let target = target.min(self.block_size);
                if self.index.get() >= target {
                    return;
                }
                // Re-entry guard: if a cycle bounces back into the same
                // wrapper while it is mid-loop, bail without doing any work.
                // `get_value_at` handles that case by returning the previous
                // slot's value.
                if self.computing.get() {
                    return;
                }
                let _prof_active = crate::profiling::push_frame(&self.id, self.mode);
                self.computing.set(true);
                // Keep adjacent to the loop below — `pop_frame` reports
                // `index - start_index` as samples processed, so any code
                // added between this read and the loop would corrupt the
                // sample count without a compiler diagnostic.
                let start_index = self.index.get();
                #process_loop
                debug_assert!(
                    unsafe { (*self.block_outputs.get()).bounds_written(self.index.get()) },
                    "{} rendered samples without their bounds",
                    self.id
                );
                self.computing.set(false);
                if _prof_active {
                    crate::profiling::pop_frame((target - start_index) as u32);
                }
            }

            fn ensure_processed(&self) {
                self.ensure_processed_to(self.block_size);
            }

            fn processed_to(&self) -> usize {
                self.index.get()
            }

            fn get_value_at(&self, port: &str, lane: usize, index: usize) -> f32 {
                let Some(port_idx) = <#block_outputs_ty>::port_index(port) else {
                    return 0.0;
                };
                let slot = self.readable_slot(index);
                let outputs = unsafe { &*self.block_outputs.get() };
                outputs.get_at(port_idx, lane, slot)
            }

            fn get_value_at_f64(&self, port: &str, lane: usize, index: usize) -> f64 {
                let Some(port_idx) = <#block_outputs_ty>::port_index(port) else {
                    return 0.0;
                };
                let slot = self.readable_slot(index);
                let outputs = unsafe { &*self.block_outputs.get() };
                outputs.get_at_f64(port_idx, lane, slot)
            }

            fn port_view(&self, port: &str) -> Option<crate::types::PortView> {
                if self.mode != crate::types::ProcessingMode::Block {
                    return None;
                }
                let port_idx = <#block_outputs_ty>::port_index(port)?;
                let outputs = unsafe { &*self.block_outputs.get() };
                let (data, lanes, words) = outputs.data_at(port_idx)?;
                if lanes == 0 {
                    return None;
                }
                Some(crate::types::PortView {
                    data,
                    lanes,
                    words,
                    processed: &self.index,
                })
            }

            fn get_bounds_at(&self, port: &str, lane: usize, index: usize) -> crate::bounds::Bounds {
                let Some(port_idx) = <#block_outputs_ty>::port_index(port) else {
                    return crate::bounds::Bounds::UNBOUNDED;
                };
                let slot = self.readable_slot(index);
                let outputs = unsafe { &*self.block_outputs.get() };
                outputs.get_bounds_at(port_idx, lane, slot)
            }

            fn port_bounds(&self, port: &str) -> crate::bounds::PortBounds {
                let Some(port_idx) = <#block_outputs_ty>::port_index(port) else {
                    return crate::bounds::PortBounds::Unbounded;
                };
                let outputs = unsafe { &*self.block_outputs.get() };
                outputs.port_bounds(
                    port_idx,
                    &self.index,
                    self.mode != crate::types::ProcessingMode::Block,
                )
            }

            fn port_index(&self, port: &str) -> Option<usize> {
                <#block_outputs_ty>::port_index(port)
            }

            fn port_names(&self) -> &'static [&'static str] {
                <#block_outputs_ty>::PORT_NAMES
            }

            fn settle_params(&self, settled: &dyn crate::bounds::Settled) {
                // SAFETY: as `bounded_outputs`.
                let params = unsafe { &mut (*self.module.get()).params };
                crate::types::BoundsInputs::settle(params, settled);
            }

            fn bounded_outputs(&self, settled: &dyn crate::bounds::Settled) -> u64 {
                // SAFETY: main thread, before the module is shipped to the
                // audio thread; nothing else holds the inner module.
                let module = unsafe { &*self.module.get() };
                <#block_outputs_ty>::settled_mask(crate::types::BoundsInputs::inputs_bounded(
                    &module.params,
                    settled,
                ))
            }

            fn ranged_outputs(&self, settled: &dyn crate::bounds::Settled) -> u64 {
                // SAFETY: as `bounded_outputs`.
                let module = unsafe { &*self.module.get() };
                <#block_outputs_ty>::settled_mask(crate::types::BoundsInputs::inputs_ranged(
                    &module.params,
                    settled,
                ))
            }

            fn bounds_sources(&self, demanded: u64) -> Vec<crate::types::CableSource> {
                // SAFETY: as `bounded_outputs`.
                let params = unsafe { &(*self.module.get()).params };
                let mut sources = Vec::new();
                let mut read = crate::types::BoundsInputs::bounds_read(params);
                if demanded & <#block_outputs_ty>::INPUT_BOUNDED_PORTS != 0 {
                    read.extend(crate::types::BoundsInputs::bounds_inputs(params));
                }
                for signal in read {
                    crate::types::Connect::collect_cables(signal, &mut sources);
                }
                sources
            }

            fn make_bounded(&self, mask: u64) {
                // SAFETY: as `bounded_outputs`.
                unsafe {
                    crate::types::OutputStruct::make_bounded(&mut (*self.module.get()).outputs, mask);
                    (*self.block_outputs.get()).make_bounded(mask);
                }
            }

            fn port_lanes(&self, port: &str) -> usize {
                let Some(port_idx) = <#block_outputs_ty>::port_index(port) else {
                    return 0;
                };
                let outputs = unsafe { &*self.block_outputs.get() };
                outputs.lanes_at(port_idx)
            }

            fn port_channels(&self, port: &str) -> usize {
                match <#block_outputs_ty>::port_index(port) {
                    Some(port_idx) if <#block_outputs_ty>::follows_lanes(port_idx) => {
                        unsafe { (*self.module.get())._channels.max(1) }
                    }
                    _ => 1,
                }
            }

            fn get_module_type(&self) -> &str {
                <#name as crate::types::Module>::MODULE_TYPE
            }

            fn get_id(&self) -> &str {
                &self.id
            }

            fn connect(&self, patch: &crate::Patch) {
                let module = unsafe { &mut *self.module.get() };
                crate::types::Connect::connect(&mut module.params, patch);
                // After resolving cables, hand each cable a back-pointer to
                // this wrapper's per-block index so it knows which sample
                // slot to read from upstream at sample-time.
                let index_ptr = &self.index as *const std::cell::Cell<usize>;
                crate::types::Connect::inject_index_ptr(&mut module.params, index_ptr);
            }

            #on_patch_update_impl

            #prepare_resources_impl

            #clock_sync_impl

            #write_module_state_impl

            fn as_any(&self) -> &dyn std::any::Any {
                self
            }

            fn get_buffer_output(&self, port: &str) -> Option<&crate::BufferData> {
                let module = unsafe { &*self.module.get() };
                crate::types::OutputStruct::get_buffer_output(&module.outputs, port)
            }

            fn transfer_state_from(
                &self,
                old: &dyn crate::types::Sampleable,
                ids: &dyn crate::types::PreviousIds,
            ) {
                if let Some(old_typed) = old.as_any().downcast_ref::<Self>() {
                    // Guard against self-aliasing: if old and new are the same box,
                    // creating two &mut refs to the same UnsafeCell contents is UB.
                    if std::ptr::eq(self as *const Self, old_typed as *const Self) {
                        return;
                    }
                    let new_inner = unsafe { &mut *self.module.get() };
                    let old_inner = unsafe { &mut *old_typed.module.get() };
                    // Block-sized output buffers and per-lane state are sized
                    // to the module's lane count, so they can only be moved
                    // between modules of the same count.
                    let same_lanes = new_inner._lane_count == old_inner._lane_count
                        && new_inner._channels == old_inner._channels;
                    #transfer_state_body
                    let generation = old_typed.glide_generation.get().wrapping_add(1);
                    self.glide_generation.set(generation);
                    crate::types::Connect::mark_glides(
                        &mut new_inner.params,
                        Some(&old_inner.params),
                        generation,
                        ids,
                    );
                    // Transfer buffer data (no-op for modules without buffer outputs)
                    crate::types::OutputStruct::transfer_buffers_from(
                        &mut new_inner.outputs,
                        &mut old_inner.outputs,
                    );
                    // Swap the block-sample buffers so feedback cycles read the
                    // previous block's values on the patch-update frame
                    // instead of the new wrapper's zero-initialised buffer.
                    // Without this swap the "second" module in a cycle would
                    // see zeros from the freshly-constructed wrapper and
                    // inject a one-sample discontinuity into the feedback loop.
                    // Skipped on a lane-count or precision change: the
                    // buffers differ in layout, and the fresh wrapper's buffer
                    // is the correct one.
                    if same_lanes {
                        unsafe {
                            let new_block_outputs = &mut *self.block_outputs.get();
                            let old_block_outputs = &mut *old_typed.block_outputs.get();
                            if new_block_outputs.same_layout(old_block_outputs) {
                                std::mem::swap(new_block_outputs, old_block_outputs);
                            }
                        }
                    }
                }
            }
        }

        fn #constructor_name(
            id: &str,
            sample_rate: f32,
            deserialized: crate::params::DeserializedParams,
            _block_size: usize,
            _mode: crate::types::ProcessingMode,
        ) -> napi::Result<Box<dyn crate::types::Sampleable>> {
            let concrete_params = deserialized.params.into_any()
                .downcast::<#params_struct_name>()
                .map_err(|_| napi::Error::from_reason(
                    format!("Failed to downcast params for module type {}", #module_name)
                ))?;

            // Construct inner module with per-field initialization.
            // `params` comes from deserialization, `_lane_count` from computed lane count,
            // all other fields use Default::default().
            let mut inner = #name #static_ty_generics {
                #(#module_field_inits),*
            };
            crate::types::OutputStruct::set_all_lanes(&mut inner.outputs, deserialized.lane_count);
            crate::types::OutputStruct::narrow_mono_outputs(&mut inner.outputs, deserialized.channels.max(1));
            crate::types::OutputStruct::make_f64_outputs(&mut inner.outputs, deserialized.f64_inputs);
            // Fill `#[default_connection]` inputs here, on the main thread:
            // the audio-thread `connect` must stay allocation-free, so it only
            // resolves connections that are already present.
            crate::types::Connect::apply_default_connections(&mut inner.params);

            let sampleable = #struct_name {
                id: id.to_owned(),
                sample_rate,
                module: std::cell::UnsafeCell::new(inner),
                index: std::cell::Cell::new(0),
                block_outputs: std::cell::UnsafeCell::new({
                    let mut ports = <#block_outputs_ty>::new(_block_size, deserialized.lane_count, deserialized.f64_inputs);
                    ports.narrow_mono_ports(deserialized.channels.max(1));
                    ports
                }),
                glide_generation: std::cell::Cell::new(0),
                block_size: _block_size,
                mode: _mode,
                f64_outputs: deserialized.f64_inputs && <#block_outputs_ty>::INHERITS,
                computing: std::cell::Cell::new(false),
            };

            #has_init_call
            Ok(Box::new(sampleable))
        }

        impl #impl_generics crate::types::Module for #name #ty_generics #where_clause {
            const MODULE_TYPE: &'static str = #module_name;

            fn install_constructor(map: &mut std::collections::HashMap<String, crate::types::SampleableConstructor>) {
                map.insert(Self::MODULE_TYPE.into(), Box::new(#constructor_name));
            }

            fn install_params_deserializer(map: &mut std::collections::HashMap<String, crate::params::ParamsDeserializer>) {
                fn deserializer(params: serde_json::Value) -> std::result::Result<crate::params::CachedParams, crate::param_errors::ModuleParamErrors> {
                    let parsed: #params_struct_name = deserr::deserialize::<_, _, crate::param_errors::ModuleParamErrors>(params)?;
                    let lane_count = #lane_count_fn_name(&parsed);
                    Ok(crate::params::CachedParams {
                        params: Box::new(parsed),
                        lane_count,
                    })
                }
                map.insert(Self::MODULE_TYPE.into(), deserializer as crate::params::ParamsDeserializer);
            }

            fn get_schema() -> crate::types::ModuleSchema {
                let params_schema = schemars::schema_for!(#params_struct_name);
                let outputs = <#outputs_ty as crate::types::OutputStruct>::schemas();

                crate::types::ModuleSchema {
                    name: Self::MODULE_TYPE.to_string(),
                    docs: crate::module_docs::ModuleDocsSchema::from(&#docs_path),
                    params_schema: crate::types::SchemaContainer {
                        schema: params_schema,
                    },
                    outputs,
                    buffer_outputs: <#outputs_ty as crate::types::OutputStruct>::buffer_port_names(),
                    signal_params: {
                        let mut params = <#params_struct_name as crate::types::SignalParamMeta>::signal_param_schemas();
                        if #stereo_mode == crate::types::StereoMode::Mono {
                            for param in &mut params {
                                param.fold = !param.reads_sides;
                            }
                        }
                        params
                    },
                    positional_args: vec![
                        #(#positional_args_exprs),*
                    ],
                    lanes: #module_lanes,
                    lanes_param: #module_lanes_param,
                    lanes_param_default: #module_lanes_param_default,
                    experimental: #experimental,
                    stereo: #stereo_mode,
                    declared_range: <#params_struct_name as crate::types::BoundsInputs>::DECLARED_RANGE
                        .map(|(min, max)| vec![min.to_string(), max.to_string()]),
                    bounding_param: <#params_struct_name as crate::types::BoundsInputs>::BOUNDING_PARAM
                        .map(|(name, values)| crate::types::BoundingParam {
                            name: name.to_string(),
                            values: values.iter().map(|v| v.to_string()).collect(),
                        }),
                }
            }
        }
    };
    Ok(generated)
}
