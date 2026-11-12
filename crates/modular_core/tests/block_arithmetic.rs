//! `$add`, `$sub`, `$mul`, `$div`, `$exp` and `_round` through every
//! block-input resolution path and the per-sample `update` path, pinned to
//! their formulas.

mod block_common;

use block_common::*;
use modular_core::types::{ProcessingMode, SameIds};
use serde_json::{Value, json};

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

type BinaryOp = fn(f32, f32) -> f32;

/// Each operator module, the name of its operand and its formula.
const OPS: [(&str, &str, BinaryOp); 5] = [
    ("$add", "offset", |x, y| x + y),
    ("$sub", "offset", |x, y| x - y),
    ("$mul", "factor", |x, y| x * y),
    ("$div", "divisor", |x, y| {
        x * if y == 0.0 { 0.0 } else { 1.0 / y }
    }),
    ("$exp", "exponent", |x, y| {
        if x == 0.0 {
            0.0
        } else {
            x.signum() * x.abs().powf(y.max(0.0))
        }
    }),
];

const MODES: [(&str, fn(f32) -> f32); 3] = [
    ("round", f32::round),
    ("floor", f32::floor),
    ("ceil", f32::ceil),
];

fn params(operand: &str, input: Value, value: Value) -> Value {
    json!({ "input": input, operand: value })
}

fn silenced(v: f32) -> f32 {
    if v.is_finite() { v } else { 0.0 }
}

#[test]
fn operations_with_a_cable_operand() {
    for (module, operand, f) in OPS {
        check_module(
            module,
            params(
                operand,
                poly_cable("src", CHANNELS),
                poly_cable("lfo", CHANNELS),
            ),
            |src, lfo| elementwise(src, lfo, f),
        );
    }
}

#[test]
fn operations_with_constant_operands() {
    const VALUES: [f32; CHANNELS] = [0.5, -2.0, 3.0, 0.25];
    for (module, operand, f) in OPS {
        check_module(
            module,
            params(operand, poly_cable("src", CHANNELS), json!(VALUES)),
            |src, _| {
                src.iter()
                    .enumerate()
                    .map(|(k, &x)| silenced(f(x, VALUES[k % CHANNELS])))
                    .collect()
            },
        );
    }
}

#[test]
fn a_mono_input_spreads_across_a_poly_operand() {
    for (module, operand, f) in OPS {
        check_module(
            module,
            params(operand, cable("src", 1), poly_cable("lfo", CHANNELS)),
            |src, lfo| {
                (0..src.len())
                    .map(|k| silenced(f(src[k / CHANNELS * CHANNELS + 1], lfo[k])))
                    .collect()
            },
        );
    }
}

#[test]
fn division_by_zero_outputs_zero() {
    check_module(
        "$div",
        params("divisor", poly_cable("src", CHANNELS), json!(0.0)),
        |src, _| vec![0.0; src.len()],
    );
}

#[test]
fn zero_to_any_power_is_zero() {
    for exponent in [0.0, 2.0] {
        check_module(
            "$exp",
            params("exponent", json!([0.0, -0.0]), json!(exponent)),
            |src, _| vec![0.0; src.len()],
        );
    }
}

#[test]
fn a_negative_exponent_holds_at_zero() {
    check_module(
        "$exp",
        params("exponent", poly_cable("src", CHANNELS), json!(-2.0)),
        |src, _| src.iter().map(|&x| x.signum()).collect(),
    );
}

#[test]
fn rounding_modes() {
    for (mode, f) in MODES {
        check_module(
            "_round",
            json!({ "input": poly_cable("src", CHANNELS), "mode": mode }),
            |src, _| src.iter().map(|&x| f(x)).collect(),
        );
    }
}

#[test]
fn paths_agree() {
    for (module, operand, _) in OPS {
        check_paths_agree(
            module,
            params(operand, poly_cable("src", CHANNELS), cable("lfo", 1)),
        );
    }
    for (mode, _) in MODES {
        check_paths_agree(
            "_round",
            json!({ "input": poly_cable("src", CHANNELS), "mode": mode }),
        );
    }
}

#[test]
fn swap_agrees_through_an_operand_glide() {
    for (module, operand, _) in OPS {
        let input = poly_cable("src", CHANNELS);
        check_swap_agrees(
            module,
            params(operand, input.clone(), json!(1.5)),
            params(operand, input, json!([3.0, 0.5])),
        );
    }
}

#[test]
fn a_divisor_edited_across_zero_glides_through_silence() {
    // From 2 to -2 the reciprocal glides from 0.5 to -0.5, so no sample is
    // louder than either end.
    let input = poly_cable("src", CHANNELS);
    check_swap_agrees(
        "$div",
        params("divisor", input.clone(), json!(2.0)),
        params("divisor", input.clone(), json!(-2.0)),
    );
    let dut = |divisor: f32| {
        build(
            ProcessingMode::Block,
            ProcessingMode::Sample,
            "$div",
            params("divisor", input.clone(), json!(divisor)),
        )
    };
    let old = dut(2.0);
    render(&old, &[BLOCK_SIZE]);
    let new = dut(-2.0);
    for (id, module) in &new.sampleables {
        module.transfer_state_from(old.sampleables[id].as_ref(), &SameIds);
    }
    new.connect_all();
    let (out, src, _) = render(&new, &[BLOCK_SIZE]);
    for (k, (&y, &x)) in out.iter().zip(&src).enumerate() {
        assert!(y.abs() <= x.abs() * 0.5 + 1e-6, "sample {k}: {y} from {x}");
    }
}

#[test]
fn allocation_free() {
    for (module, operand, _) in OPS {
        let params = params(
            operand,
            poly_cable("src", CHANNELS),
            json!([cable("lfo", 0), 2.0]),
        );
        assert_eq!(allocations_while_rendering(module, params), 0);
    }
    for (mode, _) in MODES {
        let params = json!({ "input": poly_cable("src", CHANNELS), "mode": mode });
        assert_eq!(allocations_while_rendering("_round", params), 0);
    }
}
