//! Shared engine for `$lpf`, `$hpf`, `$bpf` and `$notch`: a trapezoidal SVF
//! for the 12 dB output and its own pair of them for the 24 dB one.
//! Cutoff and resonance glide only across patch updates, so live modulation
//! reaches the filter unsmoothed.

use crate::dsp::filters::svf_core::{self, SvfCoeffs};
use crate::dsp::utils::{changed, sanitize, voct_to_hz};
use crate::poly::{PolyOutput, PolySignal, PolySignalExt};
use crate::types::SwapGlide;

#[derive(Clone, Copy)]
pub enum Response {
    Lowpass,
    Highpass,
    /// Unity gain at the center frequency.
    Bandpass,
    Notch,
}

impl Response {
    /// The SVF output mix for this response at integrator gain `g` and damping `k`.
    pub(crate) fn coeffs(self, g: f32, k: f32) -> SvfCoeffs {
        match self {
            Response::Lowpass => SvfCoeffs::new(g, k, 0.0, 0.0, 1.0),
            Response::Highpass => SvfCoeffs::new(g, k, 1.0, -k, -1.0),
            Response::Bandpass => SvfCoeffs::new(g, k, 0.0, k, 0.0),
            Response::Notch => SvfCoeffs::new(g, k, 1.0, -k, 0.0),
        }
    }
}

/// The trapezoidal integrator gain for a cutoff in V/Oct, held between
/// 20 Hz and 0.45 × `sample_rate`, in a filter running at `core_rate`.
pub(crate) fn prewarped_gain(cutoff: f32, sample_rate: f32, core_rate: f32) -> f32 {
    let freq = voct_to_hz(cutoff).min(sample_rate * 0.45).max(20.0);
    (std::f32::consts::PI * freq / core_rate).tan()
}

/// The SVF integrator gain for a cutoff in V/Oct, held between 20 Hz and
/// 0.45 × the sample rate.
pub(crate) fn cutoff_gain(cutoff: f32, sample_rate: f32) -> f32 {
    prewarped_gain(cutoff, sample_rate, sample_rate)
}

/// Which of a filter's outputs something reads: `[12 dB, 24 dB]`.
pub(crate) type Taps = [bool; 2];

/// Damping of the first of two cascaded sections that are Butterworth-flat
/// together, and the Q of the second.
pub(crate) const BUTTERWORTH_K_A: f32 = 1.847_759;
pub(crate) const BUTTERWORTH_Q_B: f32 = 1.306_563;

/// Octaves each of a 24 dB notch's two nulls sits from the centre.
pub(crate) const NOTCH_SPREAD_OCTAVES: f32 = 0.5;

/// The two sections of a fourth-order Butterworth bandpass of quality `q`:
/// their shared Q, the octaves each sits either side of the centre, and the
/// gain that brings the pair's centre response to unity.
pub(crate) fn bandpass_pair(q: f32) -> (f32, f32, f32) {
    let d = std::f32::consts::SQRT_2 / q;
    let e = 4.0 + 1.0 / (q * q);
    let section_q = ((e + (e * e - 4.0 * d * d).sqrt()) / (2.0 * d * d)).sqrt();
    let k = std::f32::consts::FRAC_1_SQRT_2 * section_q / q;
    let spread = k + (k * k - 1.0).max(0.0).sqrt();
    let detune = section_q * (spread - 1.0 / spread);
    (section_q, spread.log2(), 1.0 + detune * detune)
}

/// Coefficients of the 12 dB section and of the 24 dB output's own pair.
#[derive(Clone, Copy, Default)]
pub struct SvfSet {
    db12: SvfCoeffs,
    db24: [SvfCoeffs; 2],
    gain24: f32,
}

/// Coefficients for the outputs in `taps`; the rest stay at rest.
fn compute_coeffs(
    response: Response,
    cutoff: f32,
    resonance: f32,
    sample_rate: f32,
    taps: Taps,
) -> SvfSet {
    let q = (resonance / 5.0 * 9.0 + 0.5).max(0.5);
    let mut set = SvfSet::default();
    if taps[0] {
        set.db12 = response.coeffs(cutoff_gain(cutoff, sample_rate), 1.0 / q);
    }
    if taps[1] {
        // The pair's damping, the octaves each section sits from the
        // cutoff, and the gain after them.
        let (k, octaves, gain) = match response {
            Response::Bandpass => {
                let (section_q, octaves, gain) = bandpass_pair(q);
                ([1.0 / section_q; 2], octaves, gain)
            }
            Response::Notch => ([1.0 / q; 2], NOTCH_SPREAD_OCTAVES, 1.0),
            Response::Lowpass | Response::Highpass => (
                [BUTTERWORTH_K_A, 1.0 / (q - 0.5 + BUTTERWORTH_Q_B)],
                0.0,
                1.0,
            ),
        };
        set.db24 = if octaves == 0.0 {
            let g = cutoff_gain(cutoff, sample_rate);
            [response.coeffs(g, k[0]), response.coeffs(g, k[1])]
        } else {
            [
                response.coeffs(cutoff_gain(cutoff - octaves, sample_rate), k[0]),
                response.coeffs(cutoff_gain(cutoff + octaves, sample_rate), k[1]),
            ]
        };
        set.gain24 = gain;
    }
    set
}

/// A filter's glided cutoff and resonance.
#[derive(Clone, Copy, Default)]
pub(crate) struct CutoffResonance {
    cutoff: SwapGlide,
    resonance: SwapGlide,
}

impl CutoffResonance {
    /// Glides both params at lane `lane` and returns their current values.
    #[inline]
    pub(crate) fn update(
        &mut self,
        cutoff: &PolySignal,
        resonance: &Option<PolySignal>,
        default_resonance: f32,
        lane: usize,
        sample_rate: f32,
    ) -> (f32, f32) {
        self.cutoff
            .update(cutoff.get_value(lane), cutoff.glide_mark(lane), sample_rate);
        self.resonance.update(
            resonance.value_or(lane, default_resonance),
            resonance.glide_mark(lane),
            sample_rate,
        );
        (*self.cutoff, *self.resonance)
    }
}

/// Whether cutoff and resonance are both monophonic, so every lane can
/// share one set of coefficients.
pub(crate) fn params_mono(cutoff: &PolySignal, resonance: &Option<PolySignal>) -> bool {
    cutoff.is_monophonic() && resonance.as_ref().is_none_or(|s| s.is_monophonic())
}

/// When an edit switches between the shared and per-lane coefficient
/// paths, the newly active path takes over the other's glide state, so it
/// continues from what was actually playing.
pub(crate) fn hand_over<'a>(
    mono: bool,
    was_mono: bool,
    shared: &mut CutoffResonance,
    mut per_lane: impl Iterator<Item = &'a mut CutoffResonance>,
) {
    if mono && !was_mono {
        if let Some(first) = per_lane.next() {
            *shared = *first;
        }
    } else if !mono && was_mono {
        for glides in per_lane {
            *glides = *shared;
        }
    }
}

/// Glided params and the coefficients derived from them. `NaN` = never computed.
#[derive(Clone, Copy)]
pub struct CoeffCache<C> {
    coeffs: C,
    last_cutoff: f32,
    last_resonance: f32,
    glides: CutoffResonance,
}

impl<C: Default> Default for CoeffCache<C> {
    fn default() -> Self {
        Self {
            coeffs: C::default(),
            last_cutoff: f32::NAN,
            last_resonance: f32::NAN,
            glides: CutoffResonance::default(),
        }
    }
}

impl<C: Copy> CoeffCache<C> {
    /// Glides the params at `lane` and recomputes the coefficients with
    /// `compute(cutoff, resonance)` when either moved.
    #[inline]
    fn refresh(
        &mut self,
        params: &FilterParams,
        lane: usize,
        sample_rate: f32,
        compute: impl FnOnce(f32, f32) -> C,
    ) {
        let (c, r) = self.glides.update(
            params.cutoff,
            params.resonance,
            params.default_resonance,
            lane,
            sample_rate,
        );
        if changed(c, self.last_cutoff) || changed(r, self.last_resonance) {
            self.coeffs = compute(c, r);
            self.last_cutoff = c;
            self.last_resonance = r;
        }
    }
}

/// A filter's cutoff and resonance params.
#[derive(Clone, Copy)]
pub(crate) struct FilterParams<'a> {
    pub cutoff: &'a PolySignal,
    pub resonance: &'a Option<PolySignal>,
    pub default_resonance: f32,
}

/// A lane's state, as far as coefficient sharing reads it.
pub(crate) trait CachedLane<C> {
    fn cache(&mut self) -> &mut CoeffCache<C>;
}

/// Module-level coefficient state: the shared coefficients used when cutoff
/// and resonance are both monophonic, and the configuration they were
/// computed for.
pub struct SharedCoeffs<C, K> {
    mono: CoeffCache<C>,
    /// Whether the current params use `mono` instead of the per-lane caches.
    mono_path: bool,
    key: Option<K>,
}

impl<C: Default, K> Default for SharedCoeffs<C, K> {
    fn default() -> Self {
        Self {
            mono: CoeffCache::default(),
            mono_path: false,
            key: None,
        }
    }
}

impl<C: Copy, K: PartialEq + Copy> SharedCoeffs<C, K> {
    /// Picks the coefficient path for the new params and hands glide state
    /// over to it. `key` is everything besides cutoff and resonance that
    /// the coefficients depend on: they are recomputed when it changes.
    pub(crate) fn on_patch_update<L: CachedLane<C>>(
        &mut self,
        key: K,
        cutoff: &PolySignal,
        resonance: &Option<PolySignal>,
        lane_state: &mut [L],
    ) {
        if self.key != Some(key) {
            self.key = Some(key);
            self.mono.last_cutoff = f32::NAN;
            for lane in lane_state.iter_mut() {
                lane.cache().last_cutoff = f32::NAN;
            }
        }
        let mono = params_mono(cutoff, resonance);
        hand_over(
            mono,
            self.mono_path,
            &mut self.mono.glides,
            lane_state.iter_mut().map(|lane| &mut lane.cache().glides),
        );
        self.mono_path = mono;
    }

    /// Brings the coefficients of the first `lanes` lanes up to date.
    #[inline]
    pub(crate) fn refresh<L: CachedLane<C>>(
        &mut self,
        params: &FilterParams,
        lanes: usize,
        sample_rate: f32,
        lane_state: &mut [L],
        compute: impl Fn(f32, f32) -> C,
    ) {
        if self.mono_path {
            self.mono.refresh(params, 0, sample_rate, &compute);
        } else {
            for (i, lane) in lane_state.iter_mut().enumerate().take(lanes) {
                lane.cache().refresh(params, i, sample_rate, &compute);
            }
        }
    }

    /// The coefficients `lane` runs with.
    #[inline]
    pub(crate) fn coeffs<L: CachedLane<C>>(&self, lane: &mut L) -> C {
        if self.mono_path {
            self.mono.coeffs
        } else {
            lane.cache().coeffs
        }
    }
}

#[derive(Clone, Copy, Default, SeedSide)]
pub struct SvfLane {
    db12: [f32; 2],
    db24: [[f32; 2]; 2],
    cache: CoeffCache<SvfSet>,
}

impl CachedLane<SvfSet> for SvfLane {
    fn cache(&mut self) -> &mut CoeffCache<SvfSet> {
        &mut self.cache
    }
}

pub type SvfFilterState = SharedCoeffs<SvfSet, Taps>;

/// One engine sample of every lane. Each output in `taps` runs its own
/// sections; the other's are left as they stand.
#[allow(clippy::too_many_arguments)]
#[inline]
pub(crate) fn run(
    response: Response,
    lanes: usize,
    input: &PolySignal,
    params: &FilterParams,
    taps: Taps,
    sample_rate: f32,
    state: &mut SvfFilterState,
    lane_state: &mut [SvfLane],
    out: [&mut PolyOutput; 2],
) {
    state.refresh(params, lanes, sample_rate, lane_state, |c, r| {
        compute_coeffs(response, c, r, sample_rate, taps)
    });
    let [out12, out24] = out;
    for (i, lane) in lane_state.iter_mut().enumerate().take(lanes) {
        let c = state.coeffs(lane);
        let x = sanitize(input.get_value(i));
        if taps[0] {
            let [ic1, ic2] = &mut lane.db12;
            out12.set(i, svf_core::process(&c.db12, ic1, ic2, x));
        }
        if taps[1] {
            let [[a1, a2], [b1, b2]] = &mut lane.db24;
            let mid = svf_core::process(&c.db24[0], a1, a2, x);
            out24.set(i, c.gain24 * svf_core::process(&c.db24[1], b1, b2, mid));
        }
    }
}
