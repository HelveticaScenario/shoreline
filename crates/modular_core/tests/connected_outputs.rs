//! A settled patch tells each module which of its outputs something reads: a
//! cable, a scope or meter tap, or the engine itself. A module rebuilt on its
//! own is told what its predecessor was.

use modular_core::Patch;
use modular_core::dsp::{get_constructors, get_params_deserializers};
use modular_core::params::DeserializedParams;
use modular_core::patch::Settlement;
use modular_core::types::{ProcessingMode, Sampleable};
use serde_json::{Value, json};

fn deserialize(module_type: &str, params: Value) -> DeserializedParams {
    let cached = get_params_deserializers()[module_type](params).unwrap();
    DeserializedParams {
        params: cached.params,
        lane_count: cached.lane_count,
        f64_inputs: false,
        channels: 1,
    }
}

fn port(module: &str, port: &str) -> Value {
    json!({ "type": "cable", "module": module, "port": port, "lane": 0 })
}

fn patch(modules: &[(&str, &str, Value)]) -> (Patch, Settlement) {
    let mut patch = Patch::new();
    let settlement = patch
        .insert_modules(
            modules.iter().map(|(id, module_type, params)| {
                (
                    id.to_string(),
                    module_type.to_string(),
                    deserialize(module_type, params.clone()),
                    ProcessingMode::Block,
                )
            }),
            48_000.0,
            64,
        )
        .unwrap();
    (patch, settlement)
}

/// The ports of `module` among its connected outputs.
fn connected(module: &dyn Sampleable) -> Vec<&'static str> {
    let mask = module.connected_outputs();
    let names = module.port_names();
    (0..names.len())
        .filter(|i| mask & (1 << i) != 0)
        .map(|i| names[i])
        .collect()
}

fn filter_patch() -> Vec<(&'static str, &'static str, Value)> {
    vec![
        ("src", "$saw", json!({ "freq": 0.0 })),
        (
            "filter",
            "$multimode",
            json!({ "input": port("src", "output"), "cutoff": 1.0 }),
        ),
        (
            "reader",
            "$remap",
            json!({ "input": port("filter", "bp"), "outMin": 0.0, "outMax": 1.0 }),
        ),
        (
            "ROOT_CLOCK",
            "_clock",
            json!({ "tempo": 120.0, "numerator": 4, "denominator": 4 }),
        ),
    ]
}

#[test]
fn cables_taps_and_the_engine_connect_outputs() {
    let (patch, mut settlement) = patch(&filter_patch());
    patch.settle_connections(&mut settlement, [("filter", "hp")]);
    let module = |id: &str| patch.sampleables[id].as_ref();
    assert_eq!(connected(module("src")), ["output"]);
    assert_eq!(connected(module("filter")), ["bp", "hp"]);
    assert!(connected(module("reader")).is_empty());
    assert_eq!(module("ROOT_CLOCK").connected_outputs(), u64::MAX);
    patch.connect_all();
}

#[test]
fn an_unsettled_patch_has_every_output_connected() {
    let (patch, _) = patch(&filter_patch());
    for module in patch.sampleables.values() {
        assert_eq!(module.connected_outputs(), u64::MAX);
    }
}

#[test]
fn a_rebuilt_module_is_connected_as_its_predecessor() {
    let (patch, mut settlement) = patch(&filter_patch());
    patch.settle_connections(&mut settlement, [("filter", "hp")]);
    let constructors = get_constructors();
    let rebuild = |id: &str, module_type: &str, params: Value| {
        let mut params = deserialize(module_type, params);
        let mode = settlement.prepare_rebuilt(id, &mut params).unwrap();
        constructors[module_type](id, 48_000.0, params, 64, mode).unwrap()
    };

    let filter = rebuild(
        "filter",
        "$multimode",
        json!({ "input": port("src", "output"), "cutoff": 2.0 }),
    );
    settlement.settle_rebuilt("filter", filter.as_ref());
    assert_eq!(connected(filter.as_ref()), ["bp", "hp"]);

    let reader = rebuild(
        "reader",
        "$remap",
        json!({ "input": port("filter", "bp"), "outMin": 0.0, "outMax": 1.0 }),
    );
    assert!(settlement.disconnected_read(reader.as_ref()).is_none());
    let reader = rebuild(
        "reader",
        "$remap",
        json!({ "input": port("filter", "lp"), "outMin": 0.0, "outMax": 1.0 }),
    );
    let read = settlement.disconnected_read(reader.as_ref()).unwrap();
    assert_eq!((read.module.as_str(), read.port.as_str()), ("filter", "lp"));
}

#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "filter.lp, which the patch settled as disconnected")]
fn a_cable_to_a_disconnected_output_fails_to_connect() {
    let (mut patch, mut settlement) = patch(&filter_patch());
    patch.settle_connections(&mut settlement, []);
    let mut params = deserialize(
        "$remap",
        json!({ "input": port("filter", "lp"), "outMin": 0.0, "outMax": 1.0 }),
    );
    let mode = settlement.prepare_rebuilt("reader", &mut params).unwrap();
    let reader = get_constructors()["$remap"]("reader", 48_000.0, params, 64, mode).unwrap();
    patch.sampleables.insert("reader".into(), reader);
    patch.connect_all();
}
