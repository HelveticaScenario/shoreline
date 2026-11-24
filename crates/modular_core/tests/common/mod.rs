//! Shared integration-test helpers for `modular_core`.
//!
//! Each test file uses the parts it needs, so items unused by a given test
//! binary are expected.
#![allow(dead_code)]

use std::collections::HashMap;

use modular_core::params::{DeserializedParams, strip_argument_spans};
use modular_core::patch::Patch;
use modular_core::types::{ModuleSpec, PatchGraph, ProcessingMode};
use serde_json::Value;

/// Build a fully connected `Patch` from a `PatchGraph` for testing.
/// Mirrors what the app does for a fresh patch with no prior state to transfer
pub fn from_graph(
    graph: &PatchGraph,
    sample_rate: f32,
    block_size: usize,
    mode_map: &HashMap<String, ProcessingMode>,
) -> Result<Patch, String> {
    let deserializers = modular_core::dsp::get_params_deserializers();
    let mut modules = Vec::with_capacity(graph.modules.len());
    for module_state in &graph.modules {
        let deserializer = deserializers
            .get(&module_state.module_type)
            .ok_or_else(|| format!("Unknown module type: {}", module_state.module_type))?;
        let cached =
            deserializer(strip_argument_spans(module_state.params.clone())).map_err(|e| {
                format!(
                    "Failed to deserialize params for {}: {}",
                    module_state.id, e
                )
            })?;
        let mode = mode_map
            .get(&module_state.id)
            .copied()
            .unwrap_or(ProcessingMode::Block);
        modules.push((
            module_state.id.clone(),
            module_state.module_type.clone(),
            DeserializedParams {
                params: cached.params,
                lane_count: cached.lane_count,
                f64_inputs: false,
                channels: 1,
            },
            mode,
        ));
    }

    let mut patch = Patch::new();
    patch.insert_modules(modules, sample_rate, block_size)?;
    patch.rebuild_message_listeners();
    patch.connect_all();
    Ok(patch)
}

/// `frames` samples of every output of `read`, by port, from a patch of
/// `(id, module_type, params)` modules run one sample at a time.
pub fn render_ports(
    modules: Vec<(&str, &str, Value)>,
    sample_rate: f32,
    frames: usize,
    read: &str,
) -> Vec<(&'static str, Vec<f32>)> {
    let graph = PatchGraph {
        modules: modules
            .into_iter()
            .map(|(id, module_type, params)| ModuleSpec {
                id: id.into(),
                module_type: module_type.into(),
                id_is_explicit: None,
                skip_state_transfer: None,
                params,
            })
            .collect(),
        module_id_remaps: None,
        scopes: vec![],
        vu_meters: vec![],
    };
    let patch = from_graph(&graph, sample_rate, 1, &HashMap::new()).unwrap();
    let module = &patch.sampleables[read];
    let mut out: Vec<(&str, Vec<f32>)> = module
        .port_names()
        .iter()
        .map(|port| (*port, Vec::new()))
        .collect();
    for _ in 0..frames {
        for m in patch.sampleables.values() {
            m.start_block();
        }
        for m in patch.sampleables.values() {
            m.ensure_processed();
        }
        for (port, samples) in &mut out {
            samples.push(module.get_value_at(port, 0, 0));
        }
    }
    out
}

/// The largest magnitude among `samples`; infinite if any is not finite.
pub fn finite_peak(samples: &[f32]) -> f32 {
    samples.iter().fold(0.0f32, |peak, v| {
        if v.is_finite() {
            peak.max(v.abs())
        } else {
            f32::INFINITY
        }
    })
}
