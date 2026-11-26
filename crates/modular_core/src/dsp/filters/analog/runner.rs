//! Drives a [`Kernel`] per lane: input noise and drive, 2× oversampling
//! when `highRes` is set, and the outputs something reads.

use super::kernel::{Kernel, Rates, Taps};
use crate::dsp::filters::resonant_svf::{CachedLane, CoeffCache, FilterParams, SharedCoeffs};
use crate::dsp::utils::halfband::{Halfband2xDown, Halfband2xUp};
use crate::dsp::utils::rng::LcgRng;
use crate::dsp::utils::{changed, sanitize};
use crate::poly::{PolyOutput, PolySignal, PolySignalExt};
use crate::types::SwapGlide;

/// Peak level, in kernel units, of the noise added to the input. A circuit's
/// own noise is what starts its self-oscillation; a silent input would
/// leave the model at rest.
const NOISE_LEVEL: f32 = 1.0e-5;

/// Octaves of input gain per unit of drive: 5 is +18 dB.
const DRIVE_OCTAVES: f32 = 0.6;

/// A lane's glided drive and the gains it sets. `NaN` = never computed.
#[derive(Clone, Copy)]
struct Drive {
    glide: SwapGlide,
    last: f32,
    /// Volts to kernel units going in, and back coming out with the
    /// kernel's makeup for the drive's gain.
    input: f32,
    output: f32,
}

impl Default for Drive {
    fn default() -> Self {
        Self {
            glide: SwapGlide::default(),
            last: f32::NAN,
            input: 0.0,
            output: 0.0,
        }
    }
}

impl Drive {
    #[inline]
    fn update(
        &mut self,
        drive: &Option<PolySignal>,
        lane: usize,
        volts: f32,
        makeup: fn(f32) -> f32,
        sample_rate: f32,
    ) {
        self.glide.update(
            drive.value_or(lane, 0.0),
            drive.glide_mark(lane),
            sample_rate,
        );
        let value = *self.glide;
        if changed(value, self.last) {
            let gain = (DRIVE_OCTAVES * value.clamp(0.0, 5.0)).exp2();
            self.input = gain / volts;
            self.output = volts * makeup(gain);
            self.last = value;
        }
    }
}

#[derive(Clone, Copy, Default, SeedSide)]
pub(crate) struct AnalogLane<K: Kernel> {
    core: K::State,
    cache: CoeffCache<K::Coeffs>,
    drive: Drive,
    up: Halfband2xUp,
    down: [Halfband2xDown; 4],
    #[seed_side(keep)]
    noise: LcgRng,
}

impl<K: Kernel> AnalogLane<K> {
    /// Gives the lane its own noise stream.
    pub(crate) fn seed(&mut self, base: u64, lane: usize) {
        self.noise.seed(base, lane);
    }
}

impl<K: Kernel> CachedLane<K::Coeffs> for AnalogLane<K> {
    fn cache(&mut self) -> &mut CoeffCache<K::Coeffs> {
        &mut self.cache
    }
}

/// Everything besides cutoff and resonance a kernel's coefficients depend
/// on: the connected outputs, `highRes` and the topology's options.
pub(crate) type CoeffKey<K> = (Taps, bool, <K as Kernel>::Options);

/// Module-level state: the shared coefficients.
pub(crate) type AnalogState<K> = SharedCoeffs<<K as Kernel>::Coeffs, CoeffKey<K>>;

/// A module's outputs as the runner addresses them, by slope.
pub(crate) trait AnalogPorts: crate::types::OutputStruct {
    fn taps(connected: &Self::Connected) -> Taps;
    fn ports(&mut self) -> [Option<&mut PolyOutput>; 4];
}

/// The signals of one engine sample.
pub(crate) struct Inputs<'a> {
    pub input: &'a PolySignal,
    pub filter: FilterParams<'a>,
    pub drive: &'a Option<PolySignal>,
}

/// One engine sample of every lane, for the configuration `key`.
#[allow(clippy::too_many_arguments)]
#[inline]
pub(crate) fn run<K: Kernel>(
    response: K::Response,
    lanes: usize,
    inputs: &Inputs,
    (taps, high_res, options): CoeffKey<K>,
    sample_rate: f32,
    state: &mut AnalogState<K>,
    lane_state: &mut [AnalogLane<K>],
    mut out: [Option<&mut PolyOutput>; 4],
) {
    let rates = Rates {
        engine: sample_rate,
        core: if high_res {
            2.0 * sample_rate
        } else {
            sample_rate
        },
    };
    state.refresh(&inputs.filter, lanes, sample_rate, lane_state, |c, r| {
        K::coeffs(response, c, r, rates, taps, options)
    });
    for (i, lane) in lane_state.iter_mut().enumerate().take(lanes) {
        let c = state.coeffs(lane);
        lane.drive
            .update(inputs.drive, i, K::VOLTS, K::drive_makeup, sample_rate);
        let x = sanitize(inputs.input.get_value(i)) * lane.drive.input
            + NOISE_LEVEL * lane.noise.next_bipolar();
        let y = if high_res {
            let (first, second) = lane.up.process(x);
            let a = K::step(response, &c, &mut lane.core, first, taps);
            let b = K::step(response, &c, &mut lane.core, second, taps);
            // A downsampler nothing reads stands still.
            std::array::from_fn(|tap| {
                if taps[tap] {
                    lane.down[tap].process(a[tap], b[tap])
                } else {
                    0.0
                }
            })
        } else {
            K::step(response, &c, &mut lane.core, x, taps)
        };
        for (tap, port) in out.iter_mut().enumerate() {
            if let (true, Some(port)) = (taps[tap], port) {
                port.set(i, y[tap] * lane.drive.output);
            }
        }
    }
}
