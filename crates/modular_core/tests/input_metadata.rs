//! Every module's input and output metadata is complete and self-consistent:
//! controls carry a slider span, ranges are ordered, and descriptions leave
//! the declared range to the metadata the editor shows beside them.

use modular_core::dsp::schema;

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
            match p.signal_type.as_str() {
                "control" => {
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
            let below = p.min_value.is_some_and(|lo| p.default_value < lo);
            let above = p.max_value.is_some_and(|hi| p.default_value > hi);
            assert!(
                !below && !above,
                "{}.{}: default {} outside its range",
                module.name,
                p.name,
                p.default_value
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
            if let (Some(lo), Some(hi)) = (p.min_value, p.max_value) {
                if spellings(lo, hi).iter().any(|s| p.description.contains(s)) {
                    restated.push(format!("{}.{}: {}", module.name, p.name, p.description));
                }
            }
        }
        for o in &module.outputs {
            if let (Some(lo), Some(hi)) = (o.min_value, o.max_value) {
                if spellings(lo, hi).iter().any(|s| o.description.contains(s)) {
                    restated.push(format!(
                        "{}.{} (output): {}",
                        module.name, o.name, o.description
                    ));
                }
            }
        }
    }
    assert!(
        restated.is_empty(),
        "ranges restated in prose:\n{}",
        restated.join("\n")
    );
}
