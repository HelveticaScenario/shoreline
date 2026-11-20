//! `PhaseWarp` — a function-object param type that reshapes a 0–1 phase.
//!
//! Each variant carries `PolySignal` fields for its dynamic parameters; the consuming
//! module resolves them via `connect()`, then per sample on the audio thread reads them
//! once per lane with [`PhaseWarp::resolve`] and warps each phase with
//! [`PhaseWarp::evaluate`]. The DSL builds these with the `$phase.*` helpers.
//! Most stages are Strudel's wavetable warp modes ([`strudel`]).

pub mod shapes;
pub mod strudel;

use std::borrow::Cow;
use std::result::Result as StdResult;

use deserr::{DeserializeError, ErrorKind, IntoValue, Map as DeserrMap, ValuePointerRef};
use schemars::JsonSchema;
use serde::Serialize;

use crate::poly::PolySignal;
use crate::types::SwapGlide;
use strudel::{Mode, Prepared};

/// Most `feedback` stages one warp may contain; each needs its own slot in
/// [`PhaseWarpState`].
pub const MAX_FEEDBACK_STAGES: usize = 4;

/// Most stages with a signal one warp may contain; each needs its own slot in
/// [`PhaseWarpGlides`] and [`ResolvedWarp`].
pub const MAX_SIGNAL_STAGES: usize = 8;

/// Runtime state a [`PhaseWarp`] carries between samples: one lowpass per
/// `feedback` stage, assigned in evaluation order. Every phase the warp shapes
/// independently (a lane, an oscillator voice) needs its own copy.
#[derive(Clone, Copy, Debug, Default)]
pub struct PhaseWarpState {
    feedback: [f32; MAX_FEEDBACK_STAGES],
}

/// Which kind of stage owns a glide slot. A slot handed to a different kind
/// (the warp's shape changed in a patch edit) restarts on its new value.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum StageKind {
    #[default]
    Unused,
    Bend,
    Pulsar,
    Crush,
    Feedback,
    Strudel(Mode),
}

impl StageKind {
    /// The stage's shape constants for a knob value. Every amount is a 0–5
    /// knob (bend: -5..5).
    #[inline]
    fn prepare(self, value: f32) -> Prepared {
        match self {
            StageKind::Unused => Prepared::one(value),
            StageKind::Bend => Prepared::one(value / 5.0),
            StageKind::Pulsar => Prepared::one(shapes::pulsar_amount(value)),
            StageKind::Crush => Prepared::one(shapes::crush_amount(value)),
            StageKind::Feedback => Prepared::one(shapes::feedback_amount(value)),
            StageKind::Strudel(mode) => strudel::prepare(mode, value),
        }
    }
}

/// One [`SwapGlide`] per signal stage, assigned in evaluation order, so warp
/// amounts glide across patch edits like any other param. One copy per
/// lane: the values are per lane, not per voice.
#[derive(Clone, Copy, Debug, Default)]
pub struct PhaseWarpGlides {
    glides: [SwapGlide; MAX_SIGNAL_STAGES],
    kinds: [StageKind; MAX_SIGNAL_STAGES],
}

/// Every stage's shape constants for one lane and sample, read, glided and
/// prepared by [`PhaseWarp::resolve`], in evaluation order.
#[derive(Clone, Copy, Debug, Default)]
pub struct ResolvedWarp {
    values: [Prepared; MAX_SIGNAL_STAGES],
}

/// A warped phase and the sign to read the wave at it with: -1 where a
/// `flip` stage inverts the wave, otherwise 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Warped {
    pub phase: f32,
    pub sign: f32,
}

/// A parameterized phase warp, used by modules that read a phase (`$wavetable`,
/// `$pWarp`, `$enosc`). Serialized as an object tagged by `type`, its one
/// signal under the stage's field name; `pipe` holds `first` and `second` warps.
#[derive(Clone, Debug, Connect)]
pub enum PhaseWarp {
    Identity,
    Asym {
        amount: PolySignal,
    },
    Mirror {
        amount: PolySignal,
    },
    Bend {
        amount: PolySignal,
    },
    Sync {
        ratio: PolySignal,
    },
    Quant {
        amount: PolySignal,
    },
    Fold {
        amount: PolySignal,
    },
    Pwm {
        width: PolySignal,
    },
    Orbit {
        amount: PolySignal,
    },
    Spin {
        amount: PolySignal,
    },
    Chaos {
        amount: PolySignal,
    },
    Primes {
        amount: PolySignal,
    },
    Binary {
        amount: PolySignal,
    },
    Brownian {
        amount: PolySignal,
    },
    Reciprocal {
        amount: PolySignal,
    },
    Logistic {
        amount: PolySignal,
    },
    Sigmoid {
        amount: PolySignal,
    },
    Fractal {
        amount: PolySignal,
    },
    Flip {
        amount: PolySignal,
    },
    Pulsar {
        amount: PolySignal,
    },
    Crush {
        amount: PolySignal,
    },
    Feedback {
        amount: PolySignal,
    },
    /// Compose two warps: output phase of `first` becomes input phase of `second`.
    Pipe {
        first: Box<PhaseWarp>,
        second: Box<PhaseWarp>,
    },
}

/// Declares every stage of [`PhaseWarp`] once: its variant, its signal field,
/// its JSON `type` and its kind. Generates the stage lookup, the serialized
/// and schema forms and the deserializer's stage arms; their exhaustive
/// matches keep them in step with the enum.
macro_rules! phase_warp_stages {
    ($($variant:ident { $field:ident } = $name:literal => $kind:expr,)*) => {
        impl PhaseWarp {
            /// The stage's kind and signal; `None` for `identity` and `pipe`.
            fn stage(&self) -> Option<(StageKind, &PolySignal)> {
                match self {
                    PhaseWarp::Identity | PhaseWarp::Pipe { .. } => None,
                    $(PhaseWarp::$variant { $field } => Some(($kind, $field)),)*
                }
            }
        }

        #[derive(Serialize)]
        #[serde(tag = "type", rename_all = "camelCase")]
        enum PhaseWarpSerdeRef<'a> {
            Identity,
            $(#[serde(rename = $name)] $variant { $field: &'a PolySignal },)*
            Pipe {
                first: Box<PhaseWarpSerdeRef<'a>>,
                second: Box<PhaseWarpSerdeRef<'a>>,
            },
        }

        impl<'a> From<&'a PhaseWarp> for PhaseWarpSerdeRef<'a> {
            fn from(t: &'a PhaseWarp) -> Self {
                match t {
                    PhaseWarp::Identity => PhaseWarpSerdeRef::Identity,
                    $(PhaseWarp::$variant { $field } => PhaseWarpSerdeRef::$variant { $field },)*
                    PhaseWarp::Pipe { first, second } => PhaseWarpSerdeRef::Pipe {
                        first: Box::new(PhaseWarpSerdeRef::from(first.as_ref())),
                        second: Box::new(PhaseWarpSerdeRef::from(second.as_ref())),
                    },
                }
            }
        }

        /// The stage named `type_str`, reading its signal from `map`; `None`
        /// when no stage has that name.
        fn deserialize_stage<E: DeserializeError, V: IntoValue>(
            type_str: &str,
            map: &mut impl DeserrMap<Value = V>,
            location: ValuePointerRef<'_>,
        ) -> Option<StdResult<PhaseWarp, E>> {
            match type_str {
                $($name => Some(
                    take_signal::<E, V>(map, stringify!($field), location)
                        .map(|$field| PhaseWarp::$variant { $field }),
                ),)*
                _ => None,
            }
        }

        impl JsonSchema for PhaseWarp {
            fn schema_name() -> Cow<'static, str> {
                Cow::Borrowed("PhaseWarp")
            }

            fn json_schema(r#gen: &mut schemars::SchemaGenerator) -> schemars::Schema {
                #[derive(JsonSchema)]
                #[serde(tag = "type", rename_all = "camelCase")]
                #[allow(dead_code)]
                enum PhaseWarpSchema {
                    Identity,
                    $(#[serde(rename = $name)] $variant { $field: PolySignal },)*
                    Pipe {
                        first: Box<PhaseWarp>,
                        second: Box<PhaseWarp>,
                    },
                }
                PhaseWarpSchema::json_schema(r#gen)
            }
        }
    };
}

phase_warp_stages! {
    Asym { amount } = "asym" => StageKind::Strudel(Mode::Asym),
    Mirror { amount } = "mirror" => StageKind::Strudel(Mode::Mirror),
    Bend { amount } = "bend" => StageKind::Bend,
    Sync { ratio } = "sync" => StageKind::Strudel(Mode::Sync),
    Quant { amount } = "quant" => StageKind::Strudel(Mode::Quant),
    Fold { amount } = "fold" => StageKind::Strudel(Mode::Fold),
    Pwm { width } = "pwm" => StageKind::Strudel(Mode::Pwm),
    Orbit { amount } = "orbit" => StageKind::Strudel(Mode::Orbit),
    Spin { amount } = "spin" => StageKind::Strudel(Mode::Spin),
    Chaos { amount } = "chaos" => StageKind::Strudel(Mode::Chaos),
    Primes { amount } = "primes" => StageKind::Strudel(Mode::Primes),
    Binary { amount } = "binary" => StageKind::Strudel(Mode::Binary),
    Brownian { amount } = "brownian" => StageKind::Strudel(Mode::Brownian),
    Reciprocal { amount } = "reciprocal" => StageKind::Strudel(Mode::Reciprocal),
    Logistic { amount } = "logistic" => StageKind::Strudel(Mode::Logistic),
    Sigmoid { amount } = "sigmoid" => StageKind::Strudel(Mode::Sigmoid),
    Fractal { amount } = "fractal" => StageKind::Strudel(Mode::Fractal),
    Flip { amount } = "flip" => StageKind::Strudel(Mode::Flip),
    Pulsar { amount } = "pulsar" => StageKind::Pulsar,
    Crush { amount } = "crush" => StageKind::Crush,
    Feedback { amount } = "feedback" => StageKind::Feedback,
}

impl PhaseWarp {
    /// Read every stage's signal for `lane`, gliding values whose source
    /// changed in the last patch swap. Call once per lane and sample, then
    /// [`evaluate`](Self::evaluate) each phase the lane warps.
    ///
    /// Allocation-free — called per sample on the audio thread.
    #[inline]
    pub fn resolve(
        &self,
        lane: usize,
        sample_rate: f32,
        glides: &mut PhaseWarpGlides,
    ) -> ResolvedWarp {
        self.resolve_with(lane, sample_rate, glides, &|signal| signal.get_value(lane))
    }

    /// [`resolve`](Self::resolve) for a stereo module's lane, whose sides are
    /// `lane` and `lane + 1`: every stage reads the mean of the two sides.
    #[inline]
    pub fn resolve_folded(
        &self,
        lane: usize,
        sample_rate: f32,
        glides: &mut PhaseWarpGlides,
    ) -> ResolvedWarp {
        self.resolve_with(lane, sample_rate, glides, &|signal| {
            0.5 * (signal.get_value(lane) + signal.get_value(lane + 1))
        })
    }

    #[inline]
    fn resolve_with(
        &self,
        lane: usize,
        sample_rate: f32,
        glides: &mut PhaseWarpGlides,
        read: &impl Fn(&PolySignal) -> f32,
    ) -> ResolvedWarp {
        let mut resolved = ResolvedWarp::default();
        let mut slot = 0;
        self.resolve_from(lane, sample_rate, glides, read, &mut resolved, &mut slot);
        resolved
    }

    fn resolve_from(
        &self,
        lane: usize,
        sample_rate: f32,
        glides: &mut PhaseWarpGlides,
        read: &impl Fn(&PolySignal) -> f32,
        resolved: &mut ResolvedWarp,
        slot: &mut usize,
    ) {
        if let PhaseWarp::Pipe { first, second } = self {
            first.resolve_from(lane, sample_rate, glides, read, resolved, slot);
            second.resolve_from(lane, sample_rate, glides, read, resolved, slot);
            return;
        }
        let Some((kind, signal)) = self.stage() else {
            return;
        };
        // Deserialization caps the stage count, so the slot is in range.
        let i = *slot;
        *slot += 1;
        if glides.kinds[i] != kind {
            glides.kinds[i] = kind;
            glides.glides[i] = SwapGlide::default();
        }
        let value = glides.glides[i].follow(read(signal), signal.glide_mark(lane), sample_rate);
        resolved.values[i] = kind.prepare(value);
    }

    /// Warp phase `x` with the stage values `resolved` for its lane.
    /// `freq_norm` is the oscillator frequency over the sample rate, used to
    /// antialias the pulsar and feedback stages; pass 0 when the pitch is
    /// unknown. Multiply the wave read at the warped phase by its `sign`.
    ///
    /// Allocation-free — called per sample on the audio thread.
    #[inline]
    pub fn evaluate(
        &self,
        x: f32,
        resolved: &ResolvedWarp,
        freq_norm: f32,
        state: &mut PhaseWarpState,
    ) -> Warped {
        let (mut value_slot, mut feedback_slot) = (0, 0);
        self.evaluate_from(
            x,
            resolved,
            freq_norm,
            state,
            &mut value_slot,
            &mut feedback_slot,
        )
    }

    fn evaluate_from(
        &self,
        x: f32,
        resolved: &ResolvedWarp,
        freq_norm: f32,
        state: &mut PhaseWarpState,
        value_slot: &mut usize,
        feedback_slot: &mut usize,
    ) -> Warped {
        if let PhaseWarp::Pipe { first, second } = self {
            let a = first.evaluate_from(x, resolved, freq_norm, state, value_slot, feedback_slot);
            let b = second.evaluate_from(
                a.phase,
                resolved,
                freq_norm,
                state,
                value_slot,
                feedback_slot,
            );
            return Warped {
                phase: b.phase,
                sign: a.sign * b.sign,
            };
        }
        let Some((kind, _)) = self.stage() else {
            return Warped {
                phase: x,
                sign: 1.0,
            };
        };
        let p = &resolved.values[*value_slot];
        *value_slot += 1;
        let phase = match kind {
            StageKind::Unused => x,
            StageKind::Bend => shapes::bend(x, p.a),
            StageKind::Pulsar => shapes::pulsar(x, p.a, freq_norm),
            StageKind::Crush => shapes::crush(x, p.a),
            StageKind::Feedback => {
                let lp = &mut state.feedback[*feedback_slot];
                *feedback_slot += 1;
                shapes::feedback(x, p.a, freq_norm, lp)
            }
            StageKind::Strudel(mode) => {
                let (phase, sign) = strudel::warp(mode, x, p);
                return Warped { phase, sign };
            }
        };
        Warped { phase, sign: 1.0 }
    }

    pub fn lanes(&self) -> usize {
        match self {
            PhaseWarp::Pipe { first, second } => first.lanes().max(second.lanes()),
            _ => self.stage().map_or(0, |(_, signal)| signal.lanes()),
        }
    }

    /// The number of `feedback` stages and of stages with a signal.
    fn stage_counts(&self) -> (usize, usize) {
        match self {
            PhaseWarp::Pipe { first, second } => {
                let (a, b) = (first.stage_counts(), second.stage_counts());
                (a.0 + b.0, a.1 + b.1)
            }
            _ => match self.stage() {
                Some((StageKind::Feedback, _)) => (1, 1),
                Some(_) => (0, 1),
                None => (0, 0),
            },
        }
    }
}

impl Serialize for PhaseWarp {
    fn serialize<S>(&self, serializer: S) -> StdResult<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        PhaseWarpSerdeRef::from(self).serialize(serializer)
    }
}

fn take_field<E: DeserializeError, V: IntoValue>(
    map: &mut impl DeserrMap<Value = V>,
    field: &'static str,
    location: ValuePointerRef<'_>,
) -> StdResult<V, E> {
    map.remove(field).ok_or_else(|| {
        deserr::take_cf_content(E::error::<V>(
            None,
            ErrorKind::MissingField { field },
            location,
        ))
    })
}

fn take_signal<E: DeserializeError, V: IntoValue>(
    map: &mut impl DeserrMap<Value = V>,
    field: &'static str,
    location: ValuePointerRef<'_>,
) -> StdResult<PolySignal, E> {
    let raw = take_field::<E, V>(map, field, location)?;
    <PolySignal as deserr::Deserr<E>>::deserialize_from_value(raw.into_value(), location)
}

impl<E: DeserializeError> deserr::Deserr<E> for PhaseWarp {
    fn deserialize_from_value<V: IntoValue>(
        value: deserr::Value<V>,
        location: ValuePointerRef<'_>,
    ) -> StdResult<Self, E> {
        let unexpected = |msg: String| {
            deserr::take_cf_content(E::error::<V>(None, ErrorKind::Unexpected { msg }, location))
        };

        let mut map = match value {
            deserr::Value::Map(m) => m,
            _ => {
                return Err(unexpected(
                    "a phase warp must be an object with a \"type\" field".to_string(),
                ));
            }
        };

        let type_val = map.remove("type").and_then(|v| match v.into_value() {
            deserr::Value::String(s) => Some(s),
            _ => None,
        });

        let type_str = type_val.ok_or_else(|| {
            deserr::take_cf_content(E::error::<V>(
                None,
                ErrorKind::MissingField { field: "type" },
                location,
            ))
        })?;

        match type_str.as_str() {
            "identity" => Ok(PhaseWarp::Identity),
            "pipe" => {
                let first_raw = take_field::<E, V>(&mut map, "first", location)?;
                let second_raw = take_field::<E, V>(&mut map, "second", location)?;
                let first = PhaseWarp::deserialize_from_value(first_raw.into_value(), location)?;
                let second = PhaseWarp::deserialize_from_value(second_raw.into_value(), location)?;
                let pipe = PhaseWarp::Pipe {
                    first: Box::new(first),
                    second: Box::new(second),
                };
                // Every composition is a pipe, so checking each one bounds the whole warp.
                let (feedback, signal) = pipe.stage_counts();
                if feedback > MAX_FEEDBACK_STAGES {
                    return Err(unexpected(format!(
                        "a phase warp can contain at most {MAX_FEEDBACK_STAGES} feedback stages"
                    )));
                }
                if signal > MAX_SIGNAL_STAGES {
                    return Err(unexpected(format!(
                        "a phase warp can contain at most {MAX_SIGNAL_STAGES} stages"
                    )));
                }
                Ok(pipe)
            }
            other => deserialize_stage::<E, V>(other, &mut map, location)
                .unwrap_or_else(|| Err(unexpected(format!("unknown phase warp type: {other}")))),
        }
    }
}

#[cfg(test)]
mod tests;
