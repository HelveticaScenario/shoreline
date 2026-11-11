/// Reserved output names that cannot be used as module output port names.
/// A module's outputs are properties of the collection it returns, so these
/// are the built-in properties and methods of `BaseCollection`, `Collection`,
/// `CollectionWithRange` and `DeferredCollection`, as well as JavaScript
/// built-in property names.
///
/// This file is the single source of truth. It is `include!()`-ed by both
/// `modular_derive` (compile-time validation) and `modular` (NAPI export to JS).
/// The names are in camelCase because the TypeScript DSL converts all output names
/// to camelCase before checking. The Rust proc-macro derives snake_case variants
/// automatically from this list.
const RESERVED_OUTPUT_NAMES: &[&str] = &[
    // BaseCollection properties
    "items",
    "length",
    "channelViews",
    // BaseCollection methods
    "channel",
    "slice",
    "chunk",
    "amplitude",
    "amp",
    "exp",
    "erratic",
    "gain",
    "shift",
    "add",
    "sub",
    "mul",
    "div",
    "round",
    "floor",
    "ceil",
    "addHz",
    "mulHz",
    "scope",
    "out",
    "outMono",
    "send",
    "pipe",
    "pipeMix",
    "mix",
    "withRange",
    "toString",
    // Collection / CollectionWithRange methods
    "range",
    // DeferredCollection methods
    "set",
    // JavaScript built-in property names
    "constructor",
    "prototype",
    "__proto__",
];
