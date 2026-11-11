//! `plaits::Diffuser` on `plaits::FxEngine`: four allpasses and a damped,
//! modulated delay loop smearing the particles into a cloud, its memory
//! stored at 12 bits as in Plaits.

/// Plaits' delay lengths at 48 kHz: four allpasses, the loop's two allpasses,
/// and the loop delay.
const LENGTHS: [f32; 7] = [126.0, 180.0, 269.0, 444.0, 1653.0, 2010.0, 3411.0];
const AP4: usize = 3;
const DAPA: usize = 4;
const DAPB: usize = 5;
const DEL: usize = 6;
const KAP: f32 = 0.625;
/// Plaits' LFO: 0.3 Hz, advanced every 32 samples.
const LFO_HZ: f32 = 0.3;
const LFO_STRIDE: usize = 32;

/// Per-rate constants: delay layout and the loop's damping.
#[derive(Clone, Copy, Default)]
pub struct DiffuserRates {
    /// Each line's length and first slot, scaled to the rate.
    lengths: [usize; 7],
    bases: [usize; 7],
    size: usize,
    to_48k: f32,
    /// Plaits' 0.75 per-sample loop low-pass at this rate.
    damping: f32,
    lfo_coefficient: f32,
}

impl DiffuserRates {
    pub fn new(sample_rate: f32) -> Self {
        let to_48k = sample_rate / 48000.0;
        let mut lengths = [0; 7];
        let mut bases = [0; 7];
        let mut base = 0;
        for (i, length) in LENGTHS.iter().enumerate() {
            lengths[i] = ((length * to_48k).round() as usize).max(2);
            bases[i] = base;
            base += lengths[i] + 1;
        }
        Self {
            lengths,
            bases,
            size: base.next_power_of_two(),
            to_48k,
            damping: (1.0 - 0.25f64.powf(48000.0 / f64::from(sample_rate))) as f32,
            lfo_coefficient: cosine_coefficient(LFO_HZ / sample_rate * LFO_STRIDE as f32),
        }
    }
}

/// `stmlib::CosineOscillator`'s approximate coefficient for `frequency`
/// cycles per step.
fn cosine_coefficient(frequency: f32) -> f32 {
    let mut sign = 16.0;
    let mut frequency = frequency - 0.25;
    if frequency < 0.0 {
        frequency = -frequency;
    } else if frequency > 0.5 {
        frequency -= 0.5;
    } else {
        sign = -16.0;
    }
    sign * frequency * (1.0 - 2.0 * frequency)
}

fn compress(x: f32) -> i16 {
    ((x * 4096.0) as i32).clamp(-32768, 32767) as i16
}

fn decompress(x: i16) -> f32 {
    f32::from(x) / 4096.0
}

/// Empty until `init` replaces it with [`Diffuser::new`]; never processed
/// before then.
#[derive(Default)]
pub struct Diffuser {
    buffer: Box<[i16]>,
    mask: usize,
    write_ptr: usize,
    lp_decay: f32,
    lfo_y0: f32,
    lfo_y1: f32,
    lfo_value: f32,
}

impl Diffuser {
    /// A silent diffuser sized for `rates`. Allocates.
    pub fn new(rates: &DiffuserRates) -> Self {
        Self {
            buffer: vec![0; rates.size].into_boxed_slice(),
            mask: rates.size - 1,
            write_ptr: 0,
            lp_decay: 0.0,
            lfo_y0: 0.5,
            lfo_y1: rates.lfo_coefficient * 0.25,
            lfo_value: 0.0,
        }
    }

    #[inline(always)]
    fn at(&self, line: usize, offset: usize, rates: &DiffuserRates) -> usize {
        (self.write_ptr + rates.bases[line] + offset) & self.mask
    }

    #[inline(always)]
    fn tail(&self, line: usize, rates: &DiffuserRates) -> f32 {
        decompress(self.buffer[self.at(line, rates.lengths[line] - 1, rates)])
    }

    #[inline(always)]
    fn interpolate(&self, line: usize, offset: f32, rates: &DiffuserRates) -> f32 {
        let integral = offset as usize;
        let fractional = offset - integral as f32;
        let a = decompress(self.buffer[self.at(line, integral, rates)]);
        let b = decompress(self.buffer[self.at(line, integral + 1, rates)]);
        a + (b - a) * fractional
    }

    #[inline(always)]
    fn write(&mut self, line: usize, value: f32, rates: &DiffuserRates) {
        let at = self.at(line, 0, rates);
        self.buffer[at] = compress(value);
    }

    /// One sample of `input` mixed `amount` toward the diffused signal, the
    /// loop feeding back by `rt`.
    #[inline(always)]
    pub fn process(&mut self, amount: f32, rt: f32, input: f32, rates: &DiffuserRates) -> f32 {
        self.write_ptr = if self.write_ptr == 0 {
            self.mask
        } else {
            self.write_ptr - 1
        };
        // Plaits' FxEngine advances its LFO when its write pointer is a
        // multiple of 32.
        if self.write_ptr & (LFO_STRIDE - 1) == 0 {
            let next = self.lfo_y0;
            self.lfo_y0 = rates.lfo_coefficient * self.lfo_y0 - self.lfo_y1;
            self.lfo_y1 = next;
            self.lfo_value = next + 0.5;
        } else {
            self.lfo_value = self.lfo_y1 + 0.5;
        }
        let k = rates.to_48k;

        let mut acc = input;
        for line in 0..AP4 {
            let read = self.tail(line, rates);
            acc += read * KAP;
            self.write(line, acc, rates);
            acc = acc * -KAP + read;
        }
        let read = self.interpolate(AP4, (400.0 + 43.0 * self.lfo_value) * k, rates);
        acc += read * KAP;
        self.write(AP4, acc, rates);
        acc = acc * -KAP + read;

        acc += self.interpolate(DEL, (3070.0 + 340.0 * self.lfo_value) * k, rates) * rt;
        self.lp_decay += rates.damping * (acc - self.lp_decay);
        acc = self.lp_decay;

        let read = self.tail(DAPA, rates);
        acc += read * -KAP;
        self.write(DAPA, acc, rates);
        acc = acc * KAP + read;
        let read = self.tail(DAPB, rates);
        acc += read * KAP;
        self.write(DAPB, acc, rates);
        acc = acc * -KAP + read;

        self.write(DEL, acc, rates);
        let wet = acc * 2.0;
        input + amount * (wet - input)
    }
}
