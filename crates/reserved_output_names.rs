/// Names taken by the built-in properties and methods of `ModuleOutput`,
/// `ModuleOutputWithRange`, `BaseCollection`, `Collection`,
/// `CollectionWithRange`, `DeferredModuleOutput` and `DeferredCollection`,
/// and by JavaScript built-in properties.
///
/// A module output cannot take one of these names. Nor can the first segment
/// of a module's name after its `$`: every chainable module is also a method
/// of outputs and collections (`$lpf` as `.lpf(...)`, `$mi.chorus` as
/// `.mi.chorus(...)`), so `$gain` or `$gain.soft` would shadow `.gain(...)`.
///
/// This file is the single source of truth. It is `include!()`-ed by both
/// `modular_derive` (compile-time validation) and `modular` (NAPI export to JS).
/// The names are in camelCase because the TypeScript DSL converts all output names
/// to camelCase before checking. The Rust proc-macro derives snake_case variants
/// automatically from this list.
const RESERVED_OUTPUT_NAMES: &[&str] = &[
    // ModuleOutput properties
    "builder",
    "moduleId",
    "portName",
    "channel",
    "o",
    // ModuleOutput methods
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
    "pipe",
    "pipeMix",
    "send",
    "wet",
    "toString",
    // ModuleOutputWithRange properties
    "minValue",
    "maxValue",
    // ModuleOutputWithRange methods
    "range",
    // ModuleOutput / BaseCollection internal converter methods
    "withRange",
    "rounded",
    // BaseCollection / Collection / CollectionWithRange properties
    "items",
    "length",
    // BaseCollection methods
    "slice",
    "chunk",
    // DeferredModuleOutput / DeferredCollection properties and methods
    "set",
    "resolve",
    "resolvedModuleOutput",
    "resolving",
    // JavaScript built-in property names
    "constructor",
    "prototype",
    "__proto__",
];
