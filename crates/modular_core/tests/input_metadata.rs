//! Every module's input and output metadata is complete and self-consistent:
//! controls carry a slider span, ranges are ordered, and descriptions leave
//! the declared range to the metadata the editor shows beside them.

use modular_core::dsp::schema;
use modular_core::types::ParamType;

/// The ways a description could spell the range `lo..hi` in prose.
fn spellings(lo: f64, hi: f64) -> Vec<String> {
    let n = |v: f64| format!("{v}");
    let (a, b) = (n(lo), n(hi));
    vec![
        format!("{a}–{b}"),
        format!("{a}-{b}"),
        format!("{a} to {b}"),
        format!("{a} V to {b} V"),
        format!("{a}–{b} V"),
    ]
}

#[test]
fn control_inputs_have_a_slider_span() {
    for module in schema() {
        for p in &module.signal_params {
            let ctl = (p.control_min, p.control_max);
            match p.param_type {
                ParamType::Control => {
                    let (Some(lo), Some(hi)) = ctl else {
                        panic!("{}.{}: control without a slider span", module.name, p.name);
                    };
                    assert!(
                        lo < hi,
                        "{}.{}: slider span {lo}..{hi}",
                        module.name,
                        p.name
                    );
                }
                _ => assert_eq!(
                    ctl,
                    (None, None),
                    "{}.{}: only controls carry a slider span",
                    module.name,
                    p.name
                ),
            }
        }
    }
}

#[test]
fn ranges_are_ordered_and_hold_their_default() {
    for module in schema() {
        for p in &module.signal_params {
            if let (Some(lo), Some(hi)) = (p.min_value, p.max_value) {
                assert!(lo < hi, "{}.{}: range {lo}..{hi}", module.name, p.name);
            }
            let Some(default) = p.default_value else {
                continue;
            };
            let below = p.min_value.is_some_and(|lo| default < lo);
            let above = p.max_value.is_some_and(|hi| default > hi);
            assert!(
                !below && !above,
                "{}.{}: default {default} outside its range",
                module.name,
                p.name
            );
        }
        for o in &module.outputs {
            if let (Some(lo), Some(hi)) = (o.min_value, o.max_value) {
                assert!(
                    lo < hi,
                    "{}.{}: output range {lo}..{hi}",
                    module.name,
                    o.name
                );
            }
        }
    }
}

#[test]
fn descriptions_leave_the_range_to_the_metadata() {
    let mut restated = Vec::new();
    for module in schema() {
        for p in &module.signal_params {
            assert!(
                !p.description.trim().is_empty(),
                "{}.{}: empty description",
                module.name,
                p.name
            );
            if let (Some(lo), Some(hi)) = (p.min_value, p.max_value)
                && spellings(lo, hi).iter().any(|s| p.description.contains(s))
            {
                restated.push(format!("{}.{}: {}", module.name, p.name, p.description));
            }
        }
        for o in &module.outputs {
            if let (Some(lo), Some(hi)) = (o.min_value, o.max_value)
                && spellings(lo, hi).iter().any(|s| o.description.contains(s))
            {
                restated.push(format!(
                    "{}.{} (output): {}",
                    module.name, o.name, o.description
                ));
            }
        }
    }
    assert!(
        restated.is_empty(),
        "ranges restated in prose:\n{}",
        restated.join("\n")
    );
}

#[test]
fn defaults_live_in_the_metadata() {
    let mut in_prose = Vec::new();
    for module in schema() {
        let params = serde_json::to_value(&module.params_schema).unwrap();
        let Some(props) = params.pointer("/properties").and_then(|p| p.as_object()) else {
            continue;
        };
        for (name, prop) in props {
            let description = prop.get("description").and_then(|d| d.as_str());
            if description.is_some_and(|d| d.contains("(default")) {
                in_prose.push(format!("{}.{name}", module.name));
            }
        }
    }
    assert!(
        in_prose.is_empty(),
        "declare these defaults with `#[input(default = …)]`:\n{}",
        in_prose.join("\n")
    );
}

/// The docs list params in declaration order beside the call signature, so a
/// module's positional params are its first fields, in call order.
#[test]
fn positional_args_lead_the_params_struct() {
    let mut problems = Vec::new();
    for module in schema() {
        let params = serde_json::to_value(&module.params_schema).unwrap();
        let declared: Vec<&str> = params["properties"]
            .as_object()
            .map(|p| p.keys().map(String::as_str).collect())
            .unwrap_or_default();
        let positional: Vec<&str> = module
            .positional_args
            .iter()
            .map(|a| a.name.as_str())
            .filter(|name| declared.contains(name))
            .collect();
        if !declared.starts_with(&positional) {
            problems.push(format!(
                "{}: call order {positional:?}, struct order {declared:?}",
                module.name
            ));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
