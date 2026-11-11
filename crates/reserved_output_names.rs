/// Names taken by the built-in properties and methods of `BaseCollection`,
/// `Collection`, `CollectionWithRange` and `DeferredCollection`, and by
/// JavaScript built-in properties.
///
/// A module output cannot take one of these names. Nor can the first segment
/// of a module's name after its `$`: every chainable module is also a method
/// of collections (`$lpf` as `.lpf(...)`, `$mi.chorus` as `.mi.chorus(...)`),
/// so `$gain` or `$gain.soft` would shadow `.gain(...)`.
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
    "o",
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
    "scope",
    "out",
    "outMono",
    "send",
    "pipe",
    "pipeMix",
    "wet",
    "withRange",
    "rounded",
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
