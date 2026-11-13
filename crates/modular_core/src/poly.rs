//! Polyphonic signal support for multilane cables.
//!
//! This module provides VCV Rack-style polyphonic signal handling,
//! allowing a single cable to carry up to 16 independent audio lanes.
//!
//! - `PolyOutput`: A fixed-capacity output buffer with lane count metadata (for module outputs)
//! - `PolySignal`: A fixed-capacity input buffer containing Signal values (for polyphonic module inputs)

use crate::{
    bounds::Bounds,
    dsp::utils::sanitize,
    types::{GlideMark, PolyCableView, PreviousIds, Signal, SwapGlide},
};
use deserr::{DeserializeError, ErrorKind, IntoValue, ValuePointerRef};
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::borrow::Cow;

/// Maximum lanes per cable. A hard cap on polyphony — buffers are sized to
/// each module's actual lane count, so this only bounds the maximum, it is
/// not allocated up front.
pub const PORT_MAX_LANES: usize = 64;

/// A polyphonic output buffer holding one voltage per active lane.
///
/// The backing slice is sized to the module's lane count (1..=16) on the
/// main thread at construction (`set_lanes`), never on the audio thread.
/// `voltages.len()` is the active lane count: 0 = disconnected, 1 = mono,
/// 2-16 = polyphonic.
#[derive(Clone, Debug, PartialEq)]
pub struct PolyOutput {
    /// One voltage per active lane. Length is the lane count.
    voltages: Box<[f32]>,
    /// One full-precision value per lane on an f64 output (see
    /// [`make_f64`](Self::make_f64)); empty otherwise. `voltages` holds the
    /// same values rounded to f32.
    precise: Box<[f64]>,
    /// One [`Bounds`] per lane on a bounded output (see
    /// [`make_bounded`](Self::make_bounded)); empty otherwise.
    bounds: Box<[Bounds]>,
}

impl Default for PolyOutput {
    fn default() -> Self {
        Self {
            voltages: Box::new([]), // Disconnected (0 lanes)
            precise: Box::new([]),
            bounds: Box::new([]),
        }
    }
}

/// `value`, or 0 when it is non-finite or too large for an f32 reader.
#[inline]
fn sanitize_f64(value: f64) -> f64 {
    if (value as f32).is_finite() {
        value
    } else {
        0.0
    }
}

impl PolyOutput {
    /// Create a monophonic signal with a single value
    pub fn mono(value: f32) -> Self {
        Self {
            voltages: vec![value].into_boxed_slice(),
            precise: Box::new([]),
            bounds: Box::new([]),
        }
    }

    // === Accessors ===

    /// Get voltage for a specific lane (returns 0.0 if out of range)
    pub fn get(&self, lane: usize) -> f32 {
        self.voltages.get(lane).copied().unwrap_or(0.0)
    }

    /// Set voltage for a specific lane. An f64 output is written with
    /// [`set_f64`](Self::set_f64) instead.
    pub fn set(&mut self, lane: usize, value: f32) {
        debug_assert!(
            self.precise.is_empty(),
            "f64 outputs are written with set_f64"
        );
        if let Some(slot) = self.voltages.get_mut(lane) {
            *slot = sanitize(value);
        }
    }

    /// Set a lane at full precision on an f64 output; an f32 output keeps
    /// its nearest f32.
    pub fn set_f64(&mut self, lane: usize, value: f64) {
        let value = sanitize_f64(value);
        if let Some(slot) = self.voltages.get_mut(lane) {
            *slot = value as f32;
        }
        if let Some(slot) = self.precise.get_mut(lane) {
            *slot = value;
        }
    }

    /// Like [`get_cycling`](Self::get_cycling), at full precision on an f64
    /// output.
    pub fn get_cycling_f64(&self, lane: usize) -> f64 {
        let n = self.precise.len();
        if n == 0 {
            self.get_cycling(lane) as f64
        } else {
            self.precise[lane % n]
        }
    }

    /// Like [`get`](Self::get), at full precision on an f64 output.
    pub fn get_f64(&self, lane: usize) -> f64 {
        match self.precise.get(lane) {
            Some(&value) => value,
            None => self.get(lane) as f64,
        }
    }

    /// Make this an f64 output, holding a full-precision value per lane.
    /// Allocates — **main thread only**, after [`set_lanes`](Self::set_lanes).
    pub fn make_f64(&mut self) {
        self.precise = vec![0.0; self.voltages.len()].into_boxed_slice();
    }

    /// Whether this output holds full-precision values (see [`make_f64`](Self::make_f64)).
    pub fn is_f64(&self) -> bool {
        !self.precise.is_empty()
    }

    /// Make this a bounded output, holding a [`Bounds`] per lane beside
    /// each value. Allocates — **main thread only**, after
    /// [`set_lanes`](Self::set_lanes).
    pub fn make_bounded(&mut self) {
        self.bounds = vec![Bounds::UNBOUNDED; self.voltages.len()].into_boxed_slice();
    }

    /// Whether this output stores bounds (see [`make_bounded`](Self::make_bounded)).
    /// A module computes its bounds only when it does.
    #[inline]
    pub fn stores_bounds(&self) -> bool {
        !self.bounds.is_empty()
    }

    /// Set a lane's bounds; ignored on an output that carries none.
    #[inline]
    pub fn set_bounds(&mut self, lane: usize, bounds: Bounds) {
        if let Some(slot) = self.bounds.get_mut(lane) {
            *slot = bounds;
        }
    }

    /// Every lane's bounds; empty on an output that carries none.
    #[inline]
    pub fn bounds(&self) -> &[Bounds] {
        &self.bounds
    }

    /// Get voltage with modulo cycling: lane wraps around available lanes.
    /// This is consistent with Vec::cycle_get for non-signal params.
    /// A mono signal cycles to all lanes, a 2-lane signal alternates, etc.
    pub fn get_cycling(&self, lane: usize) -> f32 {
        let n = self.voltages.len();
        if n == 0 {
            0.0 // Disconnected
        } else {
            self.voltages[lane % n]
        }
    }

    /// Size the buffer to `lanes` active lanes.
    ///
    /// Reallocates when the count changes — **main thread only** (called once
    /// per output at construction via `OutputStruct::set_all_lanes`). When
    /// the count is unchanged this is a no-op, so it is safe to re-invoke.
    pub fn set_lanes(&mut self, lanes: usize) {
        let lanes = lanes.min(PORT_MAX_LANES);
        if self.voltages.len() != lanes {
            self.voltages = vec![0.0; lanes].into_boxed_slice();
            if !self.precise.is_empty() {
                self.precise = vec![0.0; lanes].into_boxed_slice();
            }
            if !self.bounds.is_empty() {
                self.bounds = vec![Bounds::UNBOUNDED; lanes].into_boxed_slice();
            }
        }
    }

    pub fn lanes(&self) -> usize {
        self.voltages.len()
    }
}

// === Serialization ===

impl Serialize for PolyOutput {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // Serialize as a struct with lanes and voltages array
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("PolyOutput", 2)?;
        state.serialize_field("lanes", &self.voltages.len())?;
        state.serialize_field("voltages", &self.voltages[..])?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for PolyOutput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct PolyOutputDe {
            lanes: usize,
            voltages: Vec<f32>,
        }

        let de = PolyOutputDe::deserialize(deserializer)?;
        let lanes = de.lanes.min(PORT_MAX_LANES);
        let mut voltages = vec![0.0f32; lanes].into_boxed_slice();
        for (i, &v) in de.voltages.iter().enumerate().take(lanes) {
            voltages[i] = v;
        }
        Ok(PolyOutput {
            voltages,
            precise: Box::new([]),
            bounds: Box::new([]),
        })
    }
}

// === JsonSchema ===

impl JsonSchema for PolyOutput {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("PolyOutput")
    }

    fn json_schema(r#gen: &mut schemars::SchemaGenerator) -> schemars::Schema {
        // Schema matches the serialized form
        #[derive(JsonSchema)]
        #[allow(dead_code)]
        struct PolyOutputSchema {
            lanes: usize,
            voltages: Vec<f32>,
        }
        PolyOutputSchema::json_schema(r#gen)
    }
}

// =============================================================================
// PolySignal - Polyphonic input containing multiple Signal values
// =============================================================================

/// A polyphonic input buffer containing multiple Signal values.
///
/// This is used for module inputs that need to accept polyphonic connections.
/// Each lane is a separate Signal (Volts or Cable).
/// A PolySignal always has at least 1 lane:
/// - 1 = monophonic (single signal)
/// - 2-16 = polyphonic (multiple signals)
///
/// Disconnected inputs are represented as `Option<PolySignal>` at the param level.
#[derive(Clone, Debug)]
pub struct PolySignal {
    /// Active signal lanes (always at least 1, up to PORT_MAX_LANES).
    /// Sized to the actual lane count on the main thread at deserialize.
    lanes: Box<[Signal]>,
    /// The lane values, densely packed, when every lane is a constant;
    /// empty otherwise. Read in place by block modules.
    volts: Box<[f32]>,
    /// A single view over the upstream port when lane `i` is a cable to
    /// lane `first + i` of one port, read in place by block modules. Resolved by
    /// `inject_index_ptr`, which runs after `connect`; `connect` clears it so
    /// a stale view is never read.
    cable: Option<PolyCableView>,
    /// Generation of the patch swap that installed this signal; 0 until one does.
    glide_generation: u64,
    /// Bit `i` is set when lane `i` differs from the matching lane of the
    /// replaced module's signal. All set until a swap compares them.
    changed: u64,
}

// Hand-written rather than `#[derive(Connect)]`: deriving would require a
// `Connect for Box<[Signal]>` impl, which overlaps the blanket `Connect for
// Box<T>` under coherence. Forwarding over the slice directly sidesteps that.
impl crate::types::Connect for PolySignal {
    fn apply_default_connections(&mut self) {
        // Signals carry no `#[default_connection]` fields — nothing to fill.
    }
    fn connect(&mut self, patch: &crate::Patch) {
        self.cable = None;
        for signal in self.lanes.iter_mut() {
            signal.connect(patch);
        }
    }
    fn collect_cables(&self, sink: &mut Vec<crate::types::CableSource>) {
        for signal in self.lanes.iter() {
            signal.collect_cables(sink);
        }
    }
    fn collect_lane_cables(&self, sink: &mut Vec<crate::types::CableSource>) {
        // Read lane by lane, so every cable counts (a bare `Signal` folds).
        self.collect_cables(sink);
    }
    fn inject_index_ptr(&mut self, ptr: *const std::cell::Cell<usize>) {
        for signal in self.lanes.iter_mut() {
            signal.inject_index_ptr(ptr);
        }
        self.cable = PolyCableView::resolve(&self.lanes);
    }
    fn expand_lanes(&mut self, expansion: &crate::types::LaneExpansion) {
        // Lane `l` side `c` of the reader is physical lane `l * channels + c`,
        // so cycling over the expanded lanes cycles the logical ones.
        let channels = expansion.channels;
        let mut lanes = Vec::with_capacity(self.lanes.len() * channels);
        for signal in self.lanes.iter() {
            for side in 0..channels {
                let mut signal = signal.clone();
                if let Signal::Cable {
                    module,
                    port,
                    lane,
                    fold,
                    ..
                } = &mut signal
                {
                    let producer = (expansion.producer)(module, port);
                    *lane = *lane * producer + side.min(producer - 1);
                    *fold = channels < producer;
                }
                lanes.push(signal);
            }
        }
        *self = Self::from_lanes(lanes.into_boxed_slice());
    }
    fn mark_glides(&mut self, old: Option<&Self>, generation: u64, ids: &dyn PreviousIds) {
        self.glide_generation = generation;
        self.changed = 0;
        for (i, signal) in self.lanes.iter().enumerate() {
            let same = old
                .and_then(|o| o.lanes.get(i))
                .is_some_and(|o| signal.same_source(o, ids));
            if !same {
                self.changed |= crate::types::GlideMark::bit(i);
            }
        }
    }
}

impl PolySignal {
    /// Create a mono (1-lane) PolySignal from a single Signal.
    pub fn mono(signal: Signal) -> Self {
        Self::from_lanes(vec![signal].into_boxed_slice())
    }

    /// Create a polyphonic PolySignal from a slice of Signals.
    pub fn poly(signals: &[Signal]) -> Self {
        assert!(!signals.is_empty(), "PolySignal must have at least 1 lane");
        let lanes = signals
            .iter()
            .take(PORT_MAX_LANES)
            .cloned()
            .collect::<Vec<_>>()
            .into_boxed_slice();
        Self::from_lanes(lanes)
    }

    fn from_lanes(lanes: Box<[Signal]>) -> Self {
        let volts = lanes
            .iter()
            .map(|signal| match signal {
                Signal::Volts(v) => Some(*v),
                Signal::Cable { .. } => None,
            })
            .collect::<Option<Box<[f32]>>>()
            .unwrap_or_default();
        Self {
            lanes,
            volts,
            cable: None,
            glide_generation: 0,
            changed: u64::MAX,
        }
    }

    // === Accessors ===

    /// The lane values when every lane is a constant; empty otherwise.
    pub(crate) fn volts(&self) -> &[f32] {
        &self.volts
    }

    /// Every lane's signal.
    pub(crate) fn signals(&self) -> &[Signal] {
        &self.lanes
    }

    /// The single-port view, when the lanes resolved to one.
    pub(crate) fn cable_view(&self) -> Option<&PolyCableView> {
        self.cable.as_ref()
    }

    /// Get the number of active lanes
    pub fn lanes(&self) -> usize {
        self.lanes.len()
    }

    /// Check if monophonic (exactly 1 lane)
    pub fn is_monophonic(&self) -> bool {
        self.lanes.len() == 1
    }

    /// Check if polyphonic (more than 1 lane)
    pub fn is_polyphonic(&self) -> bool {
        self.lanes.len() > 1
    }

    /// Get signal at a specific lane (returns None if out of range)
    pub fn get(&self, lane: usize) -> Option<&Signal> {
        self.lanes.get(lane)
    }

    /// Get signal with cycling (wraps around available lanes)
    pub fn get_cycling(&self, lane: usize) -> &Signal {
        &self.lanes[lane % self.lanes.len()]
    }

    /// Get the f32 value at a lane with cycling
    pub fn get_value(&self, lane: usize) -> f32 {
        self.lanes[lane % self.lanes.len()].get_value()
    }

    /// Like [`get_value`](Self::get_value), at full precision from an f64 output.
    pub fn get_value_f64(&self, lane: usize) -> f64 {
        self.lanes[lane % self.lanes.len()].get_value_f64()
    }

    /// The bounds of the value [`get_value`](Self::get_value) reads at the
    /// same slot.
    #[inline]
    pub fn get_bounds(&self, lane: usize) -> Bounds {
        self.lanes[lane % self.lanes.len()].get_bounds()
    }

    /// Lane `lane`'s bounds when they hold still over `range`, rendering a
    /// cable's producer through it; `None` when they move, so a block path
    /// declines and the slots run `update`, which reads them one by one.
    #[inline]
    pub fn block_bounds(&self, range: std::ops::Range<usize>, lane: usize) -> Option<Bounds> {
        let len = self.lanes.len();
        self.lanes[if lane < len { lane } else { lane % len }].uniform_bounds(range)
    }

    /// Whether this input is bounded in a patch as `settled` so far: every
    /// lane a constant or a cable from a bounded output. Main thread,
    /// while the patch is analysed.
    pub fn statically_bounded(&self, settled: &dyn crate::bounds::Settled) -> bool {
        self.lanes.iter().all(|signal| match signal {
            Signal::Volts(_) => true,
            Signal::Cable { module, port, .. } => settled.bounded(module, port),
        })
    }

    /// Whether this input has a range in a patch as `settled` so far: every
    /// lane a cable from a ranged output. A constant holds still, so it
    /// has none. Main thread, while the patch is analysed.
    pub fn statically_ranged(&self, settled: &dyn crate::bounds::Settled) -> bool {
        self.lanes.iter().all(|signal| match signal {
            Signal::Volts(_) => false,
            Signal::Cable { module, port, .. } => settled.ranged(module, port),
        })
    }

    /// Whether every lane is a fixed voltage.
    pub fn is_constant(&self) -> bool {
        !self.volts.is_empty()
    }

    /// The [`GlideMark`] for the lane `get_value(lane)` reads.
    #[inline]
    pub fn glide_mark(&self, lane: usize) -> GlideMark {
        GlideMark {
            generation: self.glide_generation,
            changed: self.changed,
            lane,
            lanes: self.lanes.len(),
        }
    }

    /// Whether any of `glides`, one per lane of this signal, starts or
    /// continues a glide on its next update. While none does, a block path
    /// reads the signal raw and then calls [`settle_glides`](Self::settle_glides).
    pub fn glides_pending(&self, glides: &[SwapGlide]) -> bool {
        glides
            .iter()
            .enumerate()
            .any(|(i, glide)| glide.will_glide(self.glide_mark(i)))
    }

    /// Records the last block row a block path read raw as each glide's input.
    pub fn settle_glides(&self, glides: &mut [SwapGlide], row: &[f32]) {
        for (i, (glide, &value)) in glides.iter_mut().zip(row).enumerate() {
            glide.settle(value, self.glide_mark(i));
        }
    }

    /// Calculate the maximum lane count across multiple PolySignals
    pub fn max_lanes(poly_signals: &[&PolySignal]) -> usize {
        poly_signals
            .iter()
            .map(|sig| sig.lanes.len())
            .max()
            .unwrap_or(0)
    }
}

impl From<LaneSum> for PolySignal {
    fn from(mono: LaneSum) -> PolySignal {
        mono.inner
    }
}

// === Serialization for PolySignal ===

impl Serialize for PolySignal {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Serialize as array of active signals
        let signals: Vec<&Signal> = self.lanes.iter().collect();
        signals.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for PolySignal {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // Accept either a single signal or an array of signals
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum PolySignalDe {
            Single(Signal),
            Array(Vec<Signal>),
        }

        match PolySignalDe::deserialize(deserializer)? {
            PolySignalDe::Single(signal) => Ok(PolySignal::mono(signal)),
            PolySignalDe::Array(signals) => {
                if signals.is_empty() {
                    return Err(serde::de::Error::custom(
                        "PolySignal must have at least 1 lane",
                    ));
                }
                if signals.len() > PORT_MAX_LANES {
                    return Err(serde::de::Error::custom(format!(
                        "PolySignal cannot exceed {} lanes",
                        PORT_MAX_LANES
                    )));
                }
                Ok(PolySignal::poly(&signals))
            }
        }
    }
}

// deserr implementation for PolySignal - accepts single Signal or array of Signals.
impl<E: DeserializeError> deserr::Deserr<E> for PolySignal {
    fn deserialize_from_value<V: IntoValue>(
        value: deserr::Value<V>,
        location: ValuePointerRef<'_>,
    ) -> Result<Self, E> {
        match &value {
            deserr::Value::Sequence(_) => {
                // Array of signals
                let signals =
                    <Vec<crate::types::Signal> as deserr::Deserr<E>>::deserialize_from_value(
                        value, location,
                    )?;
                if signals.is_empty() {
                    return Err(deserr::take_cf_content(E::error::<V>(
                        None,
                        ErrorKind::Unexpected {
                            msg: "PolySignal must have at least 1 lane".to_string(),
                        },
                        location,
                    )));
                }
                if signals.len() > PORT_MAX_LANES {
                    return Err(deserr::take_cf_content(E::error::<V>(
                        None,
                        ErrorKind::Unexpected {
                            msg: format!("PolySignal cannot exceed {} lanes", PORT_MAX_LANES),
                        },
                        location,
                    )));
                }
                Ok(PolySignal::poly(&signals))
            }
            _ => {
                // Single signal
                let signal = <crate::types::Signal as deserr::Deserr<E>>::deserialize_from_value(
                    value, location,
                )?;
                Ok(PolySignal::mono(signal))
            }
        }
    }
}

impl JsonSchema for PolySignal {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("PolySignal")
    }

    fn json_schema(r#gen: &mut schemars::SchemaGenerator) -> schemars::Schema {
        // Schema: either a single Signal or array of Signals
        #[derive(JsonSchema)]
        #[serde(untagged)]
        #[allow(dead_code)]
        enum PolySignalSchema {
            Single(crate::types::Signal),
            Array(Vec<crate::types::Signal>),
        }
        PolySignalSchema::json_schema(r#gen)
    }
}

// =============================================================================
// LaneSum - Polyphonic input that sums to mono output
// =============================================================================

/// A polyphonic input that sums all lanes to a single mono value.
///
/// This wraps a `PolySignal` but provides a `Signal`-like API where
/// `get_value()` returns the sum of all lanes in the PolySignal.
///
/// Use this for module inputs that accept polyphonic connections but produce
/// a mono control signal (e.g., stereo width, etc.).
///
/// Disconnected inputs are represented as `Option<LaneSum>` at the param level.
///
/// For polyphony propagation, LaneSum is treated as a single lane.
#[derive(Clone, Debug)]
pub struct LaneSum {
    inner: PolySignal,
}

// Hand-written: a LaneSum reads one value for the whole module, so it folds a
// stereo producer and never makes the module stereo.
impl crate::types::Connect for LaneSum {
    fn apply_default_connections(&mut self) {
        self.inner.apply_default_connections();
    }
    fn connect(&mut self, patch: &crate::Patch) {
        self.inner.connect(patch);
    }
    fn collect_cables(&self, sink: &mut Vec<crate::types::CableSource>) {
        self.inner.collect_cables(sink);
    }
    fn inject_index_ptr(&mut self, ptr: *const std::cell::Cell<usize>) {
        self.inner.inject_index_ptr(ptr);
    }
    fn mark_glides(&mut self, old: Option<&Self>, generation: u64, ids: &dyn PreviousIds) {
        self.inner
            .mark_glides(old.map(|o| &o.inner), generation, ids);
    }
    fn collect_lane_cables(&self, _sink: &mut Vec<crate::types::CableSource>) {}
    fn expand_lanes(&mut self, expansion: &crate::types::LaneExpansion) {
        self.inner.expand_lanes(&crate::types::LaneExpansion {
            channels: 1,
            producer: expansion.producer,
        });
    }
}

impl LaneSum {
    /// Create a LaneSum from a PolySignal
    pub fn from_poly(poly: PolySignal) -> Self {
        Self { inner: poly }
    }

    /// The lanes this input sums.
    pub fn poly(&self) -> &PolySignal {
        &self.inner
    }

    /// Get the summed value of all lanes
    pub fn get_value(&self) -> f32 {
        self.inner.lanes.iter().map(|s| s.get_value()).sum()
    }

    /// The [`GlideMark`] for the summed value: changed when any lane is.
    #[inline]
    pub fn glide_mark(&self) -> GlideMark {
        GlideMark {
            generation: self.inner.glide_generation,
            changed: if self.inner.changed != 0 { u64::MAX } else { 0 },
            lane: 0,
            lanes: 1,
        }
    }

    /// The summed value when every lane is a fixed voltage; None when any
    /// lane is a cable.
    pub fn constant_value(&self) -> Option<f32> {
        let mut sum = 0.0;
        for signal in &self.inner.lanes {
            match signal {
                Signal::Volts(v) => sum += v,
                _ => return None,
            }
        }
        Some(sum)
    }

    /// Get the summed value of all lanes as f64
    pub fn get_value_f64(&self) -> f64 {
        self.inner
            .lanes
            .iter()
            .map(Signal::get_value_f64)
            .sum::<f64>()
    }
}

impl From<PolySignal> for LaneSum {
    fn from(poly: PolySignal) -> LaneSum {
        LaneSum { inner: poly }
    }
}

// === Serialization for LaneSum (delegates to PolySignal) ===

impl Serialize for LaneSum {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.inner.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for LaneSum {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(LaneSum {
            inner: PolySignal::deserialize(deserializer)?,
        })
    }
}

// deserr implementation for LaneSum - delegates to PolySignal.
impl<E: DeserializeError> deserr::Deserr<E> for LaneSum {
    fn deserialize_from_value<V: IntoValue>(
        value: deserr::Value<V>,
        location: ValuePointerRef<'_>,
    ) -> Result<Self, E> {
        Ok(LaneSum {
            inner: PolySignal::deserialize_from_value(value, location)?,
        })
    }
}

impl JsonSchema for LaneSum {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("LaneSum")
    }

    fn json_schema(r#gen: &mut schemars::SchemaGenerator) -> schemars::Schema {
        // Reuse PolySignal's schema
        PolySignal::json_schema(r#gen)
    }
}

// =============================================================================
// PairSignal - a polyphonic input read one side at a time
// =============================================================================

/// A polyphonic input read side by side: both sides of a stereo producer,
/// and a mono producer on both. It never makes its module stereo, so a
/// module that splits or joins stereo reads its sides through it. The DSL
/// sees it as a `PolySignal`.
#[derive(Clone, Debug)]
pub struct PairSignal {
    inner: PolySignal,
    /// Set once [`expand_lanes`](crate::types::Connect::expand_lanes) has
    /// laid each lane out as two sides.
    paired: bool,
}

impl PairSignal {
    /// The underlying signal, for counting lanes before construction.
    pub(crate) fn poly(&self) -> &PolySignal {
        &self.inner
    }

    /// The input's lanes.
    pub fn lanes(&self) -> usize {
        if self.paired {
            self.inner.lanes() / 2
        } else {
            self.inner.lanes()
        }
    }

    /// Side `side` (0 left, 1 right) of lane `lane`, cycling lanes.
    #[inline]
    pub fn get_value(&self, lane: usize, side: usize) -> f32 {
        self.inner.get_value(lane * 2 + side)
    }

    /// Lane `lane` folded to mono, `(L + R) / 2`: a mono producer's value as
    /// it is.
    #[inline]
    pub fn get_folded(&self, lane: usize) -> f32 {
        0.5 * (self.get_value(lane, 0) + self.get_value(lane, 1))
    }
}

impl crate::types::Connect for PairSignal {
    fn apply_default_connections(&mut self) {
        self.inner.apply_default_connections();
    }
    fn connect(&mut self, patch: &crate::Patch) {
        self.inner.connect(patch);
    }
    fn collect_cables(&self, sink: &mut Vec<crate::types::CableSource>) {
        self.inner.collect_cables(sink);
    }
    fn inject_index_ptr(&mut self, ptr: *const std::cell::Cell<usize>) {
        self.inner.inject_index_ptr(ptr);
    }
    fn mark_glides(&mut self, old: Option<&Self>, generation: u64, ids: &dyn PreviousIds) {
        self.inner
            .mark_glides(old.map(|o| &o.inner), generation, ids);
    }
    fn collect_lane_cables(&self, _sink: &mut Vec<crate::types::CableSource>) {}
    fn expand_lanes(&mut self, expansion: &crate::types::LaneExpansion) {
        self.inner.expand_lanes(&crate::types::LaneExpansion {
            channels: 2,
            producer: expansion.producer,
        });
        self.paired = true;
    }
}

impl Serialize for PairSignal {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.inner.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for PairSignal {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(PairSignal {
            inner: PolySignal::deserialize(deserializer)?,
            paired: false,
        })
    }
}

impl<E: DeserializeError> deserr::Deserr<E> for PairSignal {
    fn deserialize_from_value<V: IntoValue>(
        value: deserr::Value<V>,
        location: ValuePointerRef<'_>,
    ) -> Result<Self, E> {
        Ok(PairSignal {
            inner: PolySignal::deserialize_from_value(value, location)?,
            paired: false,
        })
    }
}

impl JsonSchema for PairSignal {
    fn schema_name() -> Cow<'static, str> {
        PolySignal::schema_name()
    }

    fn schema_id() -> Cow<'static, str> {
        PolySignal::schema_id()
    }

    fn json_schema(r#gen: &mut schemars::SchemaGenerator) -> schemars::Schema {
        PolySignal::json_schema(r#gen)
    }
}

// =============================================================================
// Extension traits for Option<PolySignal> and Option<LaneSum>
// =============================================================================

/// Extension trait for `Option<PolySignal>` providing normalled/default value access.
///
/// This replaces the old `is_disconnected()` and `get_value_or()` methods that were
/// on `PolySignal` itself. Disconnection is represented by `None` at the param level.
pub trait PolySignalExt {
    /// Returns the signal's value at the given lane (cycling), or `default` if disconnected.
    fn value_or(&self, lane: usize, default: f32) -> f32;

    /// Returns the signal's value at the given lane (cycling), or 0.0 if disconnected.
    fn value_or_zero(&self, lane: usize) -> f32;

    /// Returns the number of active lanes, or 0 if disconnected.
    fn lane_count(&self) -> usize;

    /// Returns true if the signal is disconnected (None).
    fn is_disconnected(&self) -> bool;

    /// The [`GlideMark`] for the given lane, or [`GlideMark::ABSENT`] if disconnected.
    fn glide_mark(&self, lane: usize) -> GlideMark;
}

impl PolySignalExt for Option<PolySignal> {
    fn value_or(&self, lane: usize, default: f32) -> f32 {
        match self {
            Some(ps) => ps.get_value(lane),
            None => default,
        }
    }

    fn value_or_zero(&self, lane: usize) -> f32 {
        self.value_or(lane, 0.0)
    }

    fn lane_count(&self) -> usize {
        match self {
            Some(ps) => ps.lanes(),
            None => 0,
        }
    }

    fn is_disconnected(&self) -> bool {
        self.is_none()
    }

    #[inline]
    fn glide_mark(&self, lane: usize) -> GlideMark {
        match self {
            Some(ps) => ps.glide_mark(lane),
            None => GlideMark::ABSENT,
        }
    }
}

/// Normalled/default value access for `Option<LaneSum>`; a disconnected input
/// is `None` at the param level.
pub trait LaneSumExt {
    /// Returns the summed signal value, or `default` if disconnected.
    fn value_or(&self, default: f32) -> f32;

    /// Returns the summed signal value, or 0.0 if disconnected.
    fn value_or_zero(&self) -> f32;

    /// Returns true if the signal is disconnected (None).
    fn is_disconnected(&self) -> bool;

    /// The [`GlideMark`] for the summed value, or [`GlideMark::ABSENT`] if disconnected.
    fn glide_mark(&self) -> GlideMark;
}

impl LaneSumExt for Option<LaneSum> {
    fn value_or(&self, default: f32) -> f32 {
        match self {
            Some(ms) => ms.get_value(),
            None => default,
        }
    }

    fn value_or_zero(&self) -> f32 {
        self.value_or(0.0)
    }

    fn is_disconnected(&self) -> bool {
        self.is_none()
    }

    #[inline]
    fn glide_mark(&self) -> GlideMark {
        match self {
            Some(ms) => ms.glide_mark(),
            None => GlideMark::ABSENT,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::dsp::utils::hz_to_voct;

    use super::*;
    use crate::types::{Connect, SameIds};

    #[test]
    fn mark_glides_flags_only_changed_lanes() {
        let old = PolySignal::poly(&[Signal::Volts(1.0), Signal::Volts(2.0)]);
        let mut new =
            PolySignal::poly(&[Signal::Volts(1.0), Signal::Volts(5.0), Signal::Volts(3.0)]);
        new.mark_glides(Some(&old), 7, &SameIds);
        let changed: Vec<bool> = (0..3).map(|lane| new.glide_mark(lane).changed()).collect();
        assert_eq!(changed, [false, true, true], "a new lane counts as changed");
        assert!(new.glide_mark(4).changed(), "lane 4 reads lane 1");
        assert_eq!(new.glide_mark(0).generation, 7);

        let mut fresh = PolySignal::mono(Signal::Volts(1.0));
        fresh.mark_glides(None, 8, &SameIds);
        assert!(
            fresh.glide_mark(0).changed(),
            "no predecessor counts as changed"
        );
    }

    #[test]
    fn test_poly_signal_mono() {
        let sig = PolySignal::mono(Signal::Volts(1.0));
        assert_eq!(sig.lanes(), 1);
        assert!(sig.is_monophonic());
        assert!(!sig.is_polyphonic());
        assert_eq!(sig.get_value(0), 1.0);
    }

    #[test]
    fn test_poly_signal_poly() {
        let sig = PolySignal::poly(&[Signal::Volts(1.0), Signal::Volts(2.0), Signal::Volts(3.0)]);
        assert_eq!(sig.lanes(), 3);
        assert!(!sig.is_monophonic());
        assert!(sig.is_polyphonic());
        assert_eq!(sig.get_value(0), 1.0);
        assert_eq!(sig.get_value(1), 2.0);
        assert_eq!(sig.get_value(2), 3.0);
        // Cycling: lane 3 wraps to lane 0
        assert_eq!(sig.get_value(3), 1.0);
    }

    #[test]
    fn test_poly_signal_get_returns_option() {
        let sig = PolySignal::mono(Signal::Volts(1.0));
        assert!(sig.get(0).is_some());
        assert!(sig.get(1).is_none());
    }

    #[test]
    fn test_poly_signal_deserialize_string() {
        // Deserialize "440hz" string into PolySignal
        let json = r#""440hz""#;
        let result: PolySignal = serde_json::from_str(json).expect("Failed to deserialize");
        println!("Deserialized '440hz': lanes = {}", result.lanes());
        assert_eq!(result.lanes(), 1, "String should deserialize to 1 lane");

        let value = result.get_value(0);
        println!("Value at lane 0: {}", value);
        let target = hz_to_voct(440.0);
        assert!(
            (value - target).abs() < 0.01,
            "Value should be {} v/oct, got {}",
            target,
            value
        );
    }

    #[test]
    fn test_poly_signal_deserialize_number() {
        let json = "4.0";
        let result: PolySignal = serde_json::from_str(json).expect("Failed to deserialize");
        assert_eq!(result.lanes(), 1, "Number should deserialize to 1 lane");
        assert_eq!(result.get_value(0), 4.0);
    }

    #[test]
    fn test_poly_signal_deserialize_empty_array_fails() {
        let json = "[]";
        let result: Result<PolySignal, _> = serde_json::from_str(json);
        assert!(result.is_err(), "Empty array should fail to deserialize");
    }

    #[test]
    fn test_poly_output() {
        let mut sig = PolyOutput::default();
        sig.set_lanes(3);
        sig.set(0, 1.0);
        sig.set(1, 2.0);
        sig.set(2, 3.0);
        assert_eq!(sig.lanes(), 3);
        assert_eq!(sig.get(0), 1.0);
        assert_eq!(sig.get(1), 2.0);
        assert_eq!(sig.get(2), 3.0);
        assert_eq!(sig.get(3), 0.0);
    }

    #[test]
    fn test_poly_output_cycling() {
        let mut sig = PolyOutput::default();
        sig.set_lanes(2);
        sig.set(0, 1.0);
        sig.set(1, 2.0);
        assert_eq!(sig.get_cycling(0), 1.0);
        assert_eq!(sig.get_cycling(1), 2.0);
        assert_eq!(sig.get_cycling(2), 1.0); // wraps
        assert_eq!(sig.get_cycling(3), 2.0); // wraps
    }

    #[test]
    fn test_poly_output_disconnected() {
        let sig = PolyOutput::default();
        assert_eq!(sig.lanes(), 0);
        assert_eq!(sig.get_cycling(0), 0.0);
    }

    #[test]
    fn test_deserialize_poly() {
        use serde_json::from_str;

        // Single value
        let v: Vec<String> = from_str(r#""pink""#).map(|s: String| vec![s]).unwrap();
        assert_eq!(v, vec!["pink"]);

        // Array
        let v: Vec<String> = from_str(r#"["white", "pink", "brown"]"#).unwrap();
        assert_eq!(v, vec!["white", "pink", "brown"]);
    }

    #[test]
    fn test_mono_signal_single_lane() {
        let json = "2.5";
        let mono: LaneSum = serde_json::from_str(json).expect("Failed to deserialize");
        assert_eq!(mono.get_value(), 2.5);
    }

    #[test]
    fn test_mono_signal_sums_lanes() {
        // LaneSum should sum all lanes
        let json = "[1.0, 2.0, 3.0]";
        let mono: LaneSum = serde_json::from_str(json).expect("Failed to deserialize");
        assert_eq!(mono.get_value(), 6.0); // 1 + 2 + 3 = 6
    }

    // === Extension trait tests ===

    #[test]
    fn test_poly_signal_ext_none() {
        let opt: Option<PolySignal> = None;
        assert!(opt.is_disconnected());
        assert_eq!(opt.lane_count(), 0);
        assert_eq!(opt.value_or(0, 5.0), 5.0);
        assert_eq!(opt.value_or_zero(0), 0.0);
    }

    #[test]
    fn test_poly_signal_ext_some() {
        let opt: Option<PolySignal> = Some(PolySignal::mono(Signal::Volts(3.0)));
        assert!(!opt.is_disconnected());
        assert_eq!(opt.lane_count(), 1);
        assert_eq!(opt.value_or(0, 5.0), 3.0);
        assert_eq!(opt.value_or_zero(0), 3.0);
    }

    #[test]
    fn test_mono_signal_ext_none() {
        let opt: Option<LaneSum> = None;
        assert!(opt.is_disconnected());
        assert_eq!(opt.value_or(5.0), 5.0);
        assert_eq!(opt.value_or_zero(), 0.0);
    }

    #[test]
    fn test_mono_signal_ext_some() {
        let mono = LaneSum::from_poly(PolySignal::mono(Signal::Volts(2.0)));
        let opt: Option<LaneSum> = Some(mono);
        assert!(!opt.is_disconnected());
        assert_eq!(opt.value_or(5.0), 2.0);
        assert_eq!(opt.value_or_zero(), 2.0);
    }
}
