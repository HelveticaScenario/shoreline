//! Lowering a wire [`SeqPatternSource`] to the runtime `Pattern<SeqValue>`
//! `$cycle` plays, plus the flat per-source metadata its highlights index.
//!
//! A pattern lowers in one of two domains. Pitch patterns lower straight to
//! voltages. A scale pattern (a `.scale(...)` node, anything built on one, or
//! an arrangement whose every section is one) lowers to [`DegreeValue`]s,
//! each note carrying the scale it resolves through, and turns into voltages
//! only at its root. That is what lets `.trans` add degrees after `.fast` or
//! across an arrangement whose sections use different scales.
//!
//! Every wrapper (`.fast`, `.struct`, `$arrange`, …) lowers the same way in
//! both domains; [`Domain`] supplies the parts that differ.

use std::sync::Arc;

use crate::dsp::utilities::scale::ScaleSpec;
use crate::pattern_system::sp_combine::combine_sp;
use crate::pattern_system::{Fraction, Pattern, mini};

use super::glide_curve::GlideEase;
use super::interval_value::{EMPTY_PATTERN_SOURCE_ERR, IntervalValue};
use super::seq_value::{
    ArrangePatternPayload, BeatPatternPayload, FactorPayload, FastPatternPayload,
    GlidePatternPayload, ParsedPatternPayload, ScalePatternPayload, SeqPatternSource,
    SeqSourceMeta, SeqValue, SlowPatternPayload, StructPatternPayload, TransPatternPayload,
};

/// A lowered pattern and the sources its haps' `pattern_idx` index into.
type Lowered<V> = Result<(Pattern<V>, Vec<SeqSourceMeta>), String>;

/// Lower any wire pattern to voltages.
pub(crate) fn lower_source(source: SeqPatternSource) -> Lowered<SeqValue> {
    SeqValue::lower(source)
}

/// A note of a scale pattern before it resolves to a voltage.
#[derive(Clone, Debug)]
pub(crate) enum DegreeValue {
    Note {
        degree: i32,
        /// The scale of the nearest enclosing `.scale(...)`. Every note that
        /// reaches a scale pattern's root has one: `is_scaled` admits only
        /// trees whose every source sits under a `.scale`.
        scale: Option<Arc<ScaleSpec>>,
        glide: f64,
        ease: GlideEase,
    },
    Rest,
}

impl DegreeValue {
    fn from_interval(value: &IntervalValue) -> Self {
        match value {
            IntervalValue::Degree(degree) => DegreeValue::Note {
                degree: *degree,
                scale: None,
                glide: 0.0,
                ease: GlideEase::default(),
            },
            IntervalValue::Rest => DegreeValue::Rest,
        }
    }

    fn resolve(&self) -> SeqValue {
        match self {
            DegreeValue::Note {
                degree,
                scale: Some(scale),
                glide,
                ease,
            } => SeqValue::Voltage {
                volts: scale.degree_voltage(*degree),
                glide: *glide,
                ease: *ease,
            },
            DegreeValue::Note { scale: None, .. } | DegreeValue::Rest => SeqValue::Rest,
        }
    }
}

/// Whether `source` is a scale pattern, so it lowers to degrees.
fn is_scaled(source: &SeqPatternSource) -> bool {
    match source {
        SeqPatternSource::Scale(_) => true,
        SeqPatternSource::Trans(p) => is_scaled(&p.pattern),
        SeqPatternSource::Fast(p) => is_scaled(&p.pattern),
        SeqPatternSource::Slow(p) => is_scaled(&p.pattern),
        SeqPatternSource::Struct(p) => is_scaled(&p.pattern),
        SeqPatternSource::Beat(p) => is_scaled(&p.pattern),
        SeqPatternSource::Glide(p) => is_scaled(&p.pattern),
        SeqPatternSource::Arrange(p) => p.sections.iter().all(|s| is_scaled(&s.pattern)),
        SeqPatternSource::Single(_) => false,
    }
}

/// The value domain a pattern lowers in.
trait Domain: Clone + Send + Sync + 'static {
    /// Lower a nested pattern, the recursion every wrapper goes through.
    fn lower(source: SeqPatternSource) -> Lowered<Self>;
    /// Parse one mini-notation source.
    fn leaf(payload: ParsedPatternPayload) -> Lowered<Self>;
    fn scale(payload: ScalePatternPayload) -> Lowered<Self>;
    fn trans(payload: TransPatternPayload) -> Lowered<Self>;
    fn rest() -> Self;
    /// This note with a glide `glide` cycles long.
    fn with_glide(&self, glide: f64, ease: GlideEase) -> Self;
}

impl Domain for SeqValue {
    fn lower(source: SeqPatternSource) -> Lowered<Self> {
        if is_scaled(&source) {
            let (pattern, per_source) = DegreeValue::lower(source)?;
            Ok((pattern.fmap(DegreeValue::resolve), per_source))
        } else {
            lower_in::<SeqValue>(source)
        }
    }

    fn leaf(payload: ParsedPatternPayload) -> Lowered<Self> {
        parse_source::<SeqValue>(payload)
    }

    fn scale(_payload: ScalePatternPayload) -> Lowered<Self> {
        unreachable!("a .scale(...) node is scaled, so it lowers in the degree domain")
    }

    fn trans(_payload: TransPatternPayload) -> Lowered<Self> {
        Err(
            ".trans transposes scale degrees, so it needs a scale pattern: add .scale(...) \
             before it"
                .to_string(),
        )
    }

    fn rest() -> Self {
        SeqValue::Rest
    }

    fn with_glide(&self, glide: f64, ease: GlideEase) -> Self {
        match *self {
            SeqValue::Voltage { volts, .. } => SeqValue::Voltage { volts, glide, ease },
            SeqValue::Rest => SeqValue::Rest,
        }
    }
}

impl Domain for DegreeValue {
    fn lower(source: SeqPatternSource) -> Lowered<Self> {
        lower_in::<DegreeValue>(source)
    }

    fn leaf(payload: ParsedPatternPayload) -> Lowered<Self> {
        let (pattern, per_source) = parse_source::<IntervalValue>(payload)?;
        Ok((pattern.fmap(DegreeValue::from_interval), per_source))
    }

    /// Give every note of the inner pattern this scale, replacing any scale
    /// an inner `.scale(...)` gave it.
    fn scale(payload: ScalePatternPayload) -> Lowered<Self> {
        let scale = ScaleSpec::parse(&payload.scale)
            .map_err(|reason| format!("invalid scale \"{}\": {reason}", payload.scale))?;
        let scale = Arc::new(scale);
        let (inner, per_source) = Self::lower(*payload.pattern)?;
        let scaled = inner.fmap(move |value| match value {
            DegreeValue::Note {
                degree,
                glide,
                ease,
                ..
            } => DegreeValue::Note {
                degree: *degree,
                scale: Some(Arc::clone(&scale)),
                glide: *glide,
                ease: *ease,
            },
            DegreeValue::Rest => DegreeValue::Rest,
        });
        Ok((scaled, per_source))
    }

    /// Add the `rhs` degrees to each note, aligned by `mode`. A note keeps its
    /// own scale and glide.
    fn trans(payload: TransPatternPayload) -> Lowered<Self> {
        let (lhs, mut per_source) = Self::lower(*payload.pattern)?;
        let (rhs, rhs_meta) = parse_source::<IntervalValue>(payload.rhs)?;
        let rhs = rhs.offset_pattern_idx(per_source.len() as u32);
        per_source.extend(rhs_meta);
        let combined = combine_sp(&lhs, &rhs, payload.mode.to_rt(), |note, step| {
            match (note, step.degree()) {
                (
                    DegreeValue::Note {
                        degree,
                        scale,
                        glide,
                        ease,
                    },
                    Some(step),
                ) => DegreeValue::Note {
                    degree: degree + step,
                    scale: scale.clone(),
                    glide: *glide,
                    ease: *ease,
                },
                _ => DegreeValue::Rest,
            }
        });
        Ok((combined, per_source))
    }

    fn rest() -> Self {
        DegreeValue::Rest
    }

    fn with_glide(&self, glide: f64, ease: GlideEase) -> Self {
        match self {
            DegreeValue::Note { degree, scale, .. } => DegreeValue::Note {
                degree: *degree,
                scale: scale.clone(),
                glide,
                ease,
            },
            DegreeValue::Rest => DegreeValue::Rest,
        }
    }
}

/// Lower `source` in domain `V`, recursing through `V::lower`.
fn lower_in<V: Domain>(source: SeqPatternSource) -> Lowered<V> {
    match source {
        SeqPatternSource::Single(p) => V::leaf(p),
        SeqPatternSource::Scale(p) => V::scale(p),
        SeqPatternSource::Trans(p) => V::trans(p),
        SeqPatternSource::Fast(p) => lower_fast::<V>(p),
        SeqPatternSource::Slow(p) => lower_slow::<V>(p),
        SeqPatternSource::Struct(p) => lower_struct::<V>(p),
        SeqPatternSource::Beat(p) => lower_beat::<V>(p),
        SeqPatternSource::Glide(p) => lower_glide::<V>(p),
        SeqPatternSource::Arrange(p) => lower_arrange::<V>(p),
    }
}

/// Parse one mini-notation source into a pattern of `T` and its one metadata
/// entry. Modifier spans are stripped so euclidean operands and patterned
/// `*`/`/` factors highlight as part of this source, at `pattern_idx` 0.
fn parse_source<T>(payload: ParsedPatternPayload) -> Lowered<T>
where
    T: mini::FromMiniAtom + Clone + Send + Sync + 'static,
{
    if payload.source.trim().is_empty() {
        return Err(EMPTY_PATTERN_SOURCE_ERR.to_string());
    }
    let pattern = mini::convert::<T>(&payload.ast)
        .map_err(|e| e.to_string())?
        .strip_modifier_spans();
    let per_source = vec![SeqSourceMeta {
        source: payload.source,
        all_spans: payload.all_spans,
    }];
    Ok((pattern, per_source))
}

/// Lower an `$arrange(...)` payload: lower each section, offset its pattern
/// indices to its slot in the flat `per_source` list, and stitch the sections
/// in time. Cycle counts are positive integers, or a single trailing
/// `Infinity`.
fn lower_arrange<V: Domain>(payload: ArrangePatternPayload) -> Lowered<V> {
    if payload.sections.is_empty() {
        return Err("$arrange requires at least one section".to_string());
    }
    let section_count = payload.sections.len();
    let mut per_source: Vec<SeqSourceMeta> = Vec::new();
    let mut sections: Vec<(Option<Fraction>, Pattern<V>)> = Vec::with_capacity(section_count);

    for (i, section) in payload.sections.into_iter().enumerate() {
        let cycles = section.cycles.to_finite_cycles()?;
        if cycles.is_none() && i + 1 != section_count {
            return Err(
                "$arrange: an Infinity section must be the last section (sections after it \
                 can never play)"
                    .to_string(),
            );
        }
        let (mut pat, metas) = V::lower(section.pattern)?;
        let offset = per_source.len();
        if offset > 0 {
            pat = pat.offset_pattern_idx(offset as u32);
        }
        per_source.extend(metas);
        sections.push((cycles, pat));
    }

    Ok((crate::pattern_system::arrange(sections), per_source))
}

/// Lower a `.fast(...)` wrapper. A pattern factor (`.fast("2 4")`) is its own
/// highlightable source after the inner ones, so the active factor lights up
/// as it drives the speed; a constant factor adds no source.
fn lower_fast<V: Domain>(payload: FastPatternPayload) -> Lowered<V> {
    let (inner, mut per_source) = V::lower(*payload.pattern)?;
    let factor = lower_factor(payload.factor, &mut per_source)?;
    Ok((inner.fast(factor), per_source))
}

/// Lower a `.slow(...)` wrapper, the time-inverse of [`lower_fast`].
fn lower_slow<V: Domain>(payload: SlowPatternPayload) -> Lowered<V> {
    let (inner, mut per_source) = V::lower(*payload.pattern)?;
    let factor = lower_factor(payload.factor, &mut per_source)?;
    Ok((inner.slow(factor), per_source))
}

/// Lower a `.struct(...)` wrapper: impose the boolean pattern's timing on the
/// inner values. The bool pattern is always its own highlightable source.
fn lower_struct<V: Domain>(payload: StructPatternPayload) -> Lowered<V> {
    let (inner, mut per_source) = V::lower(*payload.pattern)?;
    let (bool_pat, bool_meta) = parse_source::<bool>(payload.bool_pattern)?;
    let bool_pat = bool_pat.offset_pattern_idx(per_source.len() as u32);
    per_source.extend(bool_meta);
    Ok((inner.struct_with_rest(&bool_pat, V::rest()), per_source))
}

/// Lower a `.beat(...)` wrapper: place the inner values on the beat grid.
/// String arguments are sources in the TS span order, `t` then `div`.
fn lower_beat<V: Domain>(payload: BeatPatternPayload) -> Lowered<V> {
    let (inner, mut per_source) = V::lower(*payload.pattern)?;
    let t = lower_factor(payload.t, &mut per_source)?;
    let div = lower_factor(payload.div, &mut per_source)?;
    Ok((inner.beat_with(t, div), per_source))
}

/// Lower a `.glide(...)` wrapper: give each note the glide length at its
/// onset, or zero where the length pattern is silent. Notes keep their own
/// timing.
fn lower_glide<V: Domain>(payload: GlidePatternPayload) -> Lowered<V> {
    let (inner, mut per_source) = V::lower(*payload.pattern)?;
    let length = lower_factor(payload.length, &mut per_source)?;
    let ease = GlideEase {
        curve: payload.curve,
        mode: payload.mode,
    };
    let glided = inner.app_onset(&length, move |value: &V, length| {
        value.with_glide(length.map_or(0.0, |l| l.to_f64().max(0.0)), ease)
    });
    Ok((glided, per_source))
}

/// Lower a numeric pattern-method argument (`.fast`/`.slow` factor, `.beat`
/// t/div, `.glide` length). A constant is span-free; a pattern string becomes
/// a source appended to `per_source`, its highlights shifted to that slot.
fn lower_factor(
    factor: FactorPayload,
    per_source: &mut Vec<SeqSourceMeta>,
) -> Result<Pattern<Fraction>, String> {
    match factor {
        FactorPayload::Const(n) => {
            if !n.is_finite() {
                return Err("pattern method argument must be a finite number".to_string());
            }
            Ok(crate::pattern_system::pure(Fraction::from(n)))
        }
        FactorPayload::Pattern(payload) => {
            let (pattern, meta) = parse_source::<f64>(payload)?;
            let pattern = pattern
                .fmap(|v| Fraction::from(*v))
                .offset_pattern_idx(per_source.len() as u32);
            per_source.extend(meta);
            Ok(pattern)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lower_factor_rejects_non_finite_const() {
        // JSON cannot carry NaN or ∞, so the guard is exercised directly.
        // `Fraction::from(f64)` maps a non-finite value to 0, which would
        // otherwise pass as factor 0 (silence).
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(
                lower_factor(FactorPayload::Const(bad), &mut Vec::new()).is_err(),
                "non-finite const factor {bad} must be rejected"
            );
        }
    }
}
