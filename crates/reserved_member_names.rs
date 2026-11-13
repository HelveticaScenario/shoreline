/// Names taken by the built-in properties and methods of `BaseCollection`,
/// `Collection`, `CollectionWithRange` and `DeferredCollection`, by the
/// methods of patterns, and by JavaScript built-in properties.
///
/// The first segment of a module's name after its `$` cannot be one of them:
/// every chainable module is also a method of collections (`$lpf` as
/// `.lpf(...)`, `$mi.chorus` as `.mi.chorus(...)`), so `$out` or `$out.soft`
/// would shadow `.out(...)`. A pattern has every collection member beside its
/// own methods, so those are taken too; `$cycle` alone keeps its name, being
/// what a pattern's `.cycle(...)` calls. A module's outputs sit behind an
/// underscore (`seq._gate`), so an output may take any of these names.
///
/// This file is the single source of truth. It is `include!()`-ed by both
/// `modular_derive` (compile-time validation) and `modular` (NAPI export to JS).
const RESERVED_MEMBER_NAMES: &[&str] = &[
    // BaseCollection properties
    "items",
    "length",
    "laneViews",
    // BaseCollection methods
    "lane",
    "slice",
    "chunk",
    "scope",
    "out",
    "outMono",
    "send",
    "pipe",
    "pipeMix",
    "wet",
    "toString",
    // Collection / CollectionWithRange methods
    "range",
    // DeferredCollection methods
    "set",
    // Pattern methods
    "cycle",
    "fast",
    "slow",
    "struct",
    "beat",
    "glide",
    "trans",
    // JavaScript built-in property names
    "constructor",
    "prototype",
    "__proto__",
];
