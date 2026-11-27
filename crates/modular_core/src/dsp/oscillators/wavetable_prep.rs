//! FFT-based mipmap generation for anti-aliased wavetable playback.
//!
//! [`PreparedWavetable`] takes a [`WavData`], splits each side into equal-size
//! frames, and generates a band-limited mipmap pyramid — each level halves the spectral
//! bandwidth of the one before it, down to the fundamental alone. At read
//! time, the oscillator plays the brightest level whose harmonics all stay
//! below Nyquist at its playback frequency, and fades into the next level (or,
//! past the last, into silence) over the last whole tone before that limit, so
//! a pitch sweep never aliases and its brightness never jumps.
//!
//! Construction is main-thread only and allocates freely. [`read`] and
//! [`mipmap_for_freq`] are allocation-free and suitable for the audio thread.
//!
//! [`read`]: PreparedWavetable::read
//! [`mipmap_for_freq`]: PreparedWavetable::mipmap_for_freq

use rustfft::FftPlanner;
use rustfft::num_complex::Complex;

use crate::types::WavData;

/// Fallback frame size used when the WAV has no detected frame size or one
/// too short to hold a harmonic (under 4 samples).
const DEFAULT_FRAME_SIZE: usize = 2048;

/// Oversampling factor for mipmap tables. The IFFT output is computed at
/// `frame_size * OVERSAMPLE` samples per frame, giving the linear interpolation
/// in `read_sample` much more accuracy without needing higher-order filters.
const OVERSAMPLE: usize = 8;

/// Width of each level's fade below its limit: a whole tone.
const FADE_OCTAVES: f32 = 1.0 / 6.0;

/// The mipmap levels to read at a playback frequency: `level`, blended toward
/// `level + 1` (silence past the last level) by `next_weight` in `[0, 1]`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct MipmapRead {
    pub level: usize,
    pub next_weight: f32,
}

/// One band-limited copy of every frame of every side.
#[derive(Debug, Clone, Connect)]
struct MipmapLevel {
    /// `sides * frame_count * table_size` samples: each side's frames stored
    /// back-to-back, the sides one after the other.
    samples: Vec<f32>,
    /// `log2` of the playback frequency in Hz below which all of this level's
    /// harmonics stay below Nyquist.
    limit: f32,
}

/// A WAV loaded and prepared for band-limited wavetable playback.
///
/// Stores a mipmap pyramid of time-domain samples, one level per spectral
/// halving, from full bandwidth at level 0 to the fundamental alone. All
/// levels share the same frame count and table size.
#[derive(Debug, Clone, Connect)]
pub struct PreparedWavetable {
    levels: Vec<MipmapLevel>,
    /// Original samples per frame from the WAV metadata.
    pub frame_size: usize,
    /// Samples per frame in the stored tables (`frame_size * OVERSAMPLE`).
    pub table_size: usize,
    /// Number of frames in the table.
    pub frame_count: usize,
    /// Sides stored: 2 for a two-channel file, otherwise 1.
    pub sides: usize,
}

impl PreparedWavetable {
    /// Prepare a wavetable from a loaded WAV.
    ///
    /// Reads both channels of a two-channel file and channel 0 of any other,
    /// each side cut into the same frames. The frame size is taken from `wav.detected_frame_size`,
    /// falling back to [`DEFAULT_FRAME_SIZE`] when the metadata is absent or
    /// degenerate. Frames past the end of the channel (i.e. if the sample
    /// count isn't a clean multiple of `frame_size`) are discarded.
    ///
    /// Main thread only — allocates and runs FFTs.
    pub fn from_wav_data(wav: &WavData, sample_rate: f32) -> Self {
        let mut frame_size = wav.detected_frame_size.unwrap_or(DEFAULT_FRAME_SIZE);
        if frame_size < 4 {
            frame_size = DEFAULT_FRAME_SIZE;
        }

        let total_samples = wav.frame_count();
        let frame_count = total_samples / frame_size;
        let table_size = frame_size * OVERSAMPLE;

        // Levels down to the last one that keeps the fundamental: level `l`
        // keeps `frame_size / 2^(l+1)` bins, at least 2 while `l <= log2 - 2`.
        let mipmap_count = integer_log2(frame_size) - 1;

        let sides = if wav.channel_count() == 2 { 2 } else { 1 };

        let nyquist = sample_rate / 2.0;
        let mut levels: Vec<MipmapLevel> = (0..mipmap_count)
            .map(|level| MipmapLevel {
                samples: vec![0.0f32; sides * frame_count * table_size],
                limit: (nyquist / (level_cutoff(frame_size, level) - 1) as f32).log2(),
            })
            .collect();

        if total_samples == 0 || wav.channel_count() == 0 || frame_count == 0 {
            return Self {
                levels,
                frame_size,
                table_size,
                frame_count,
                sides,
            };
        }

        let mut planner = FftPlanner::<f32>::new();
        let fft_forward = planner.plan_fft_forward(frame_size);
        let fft_inverse = planner.plan_fft_inverse(table_size);

        // Scratch buffers reused across frames.
        let mut spectrum: Vec<Complex<f32>> = vec![Complex::new(0.0, 0.0); frame_size];
        let mut padded: Vec<Complex<f32>> = vec![Complex::new(0.0, 0.0); table_size];
        // The forward FFT (size frame_size) leaves bins scaled by frame_size.
        // The inverse FFT (size table_size) is unnormalized in rustfft.
        // To recover the original amplitude we divide by frame_size.
        let inv_n = 1.0 / frame_size as f32;

        for (side, frame_idx) in
            (0..sides).flat_map(|side| (0..frame_count).map(move |f| (side, f)))
        {
            let src_base = frame_idx * frame_size;
            for (i, bin) in spectrum.iter_mut().enumerate() {
                *bin = Complex::new(wav.read(side, src_base + i), 0.0);
            }

            fft_forward.process(&mut spectrum);

            // Level 0 = full bandwidth. For each subsequent level, zero bins
            // at index >= frame_size / (2 << l). Mirror to the negative-
            // frequency half so the IFFT produces real output.
            //
            // The forward FFT produces `frame_size` bins. We zero-pad the
            // spectrum to `table_size` bins for the inverse FFT, placing the
            // positive-frequency half at the start and the negative-frequency
            // half at the end, with zeros in the middle. This is the standard
            // zero-padded IFFT oversampling approach.
            for (level, level_table) in levels.iter_mut().enumerate() {
                // Clear the padded buffer.
                for c in padded.iter_mut() {
                    *c = Complex::new(0.0, 0.0);
                }

                let cutoff = level_cutoff(frame_size, level);

                // Copy positive-frequency bins [0..cutoff) to padded[0..cutoff).
                padded[..cutoff].copy_from_slice(&spectrum[..cutoff]);

                // Copy negative-frequency bins. In the original frame_size FFT,
                // negative frequencies are at indices [frame_size-cutoff+1 .. frame_size-1].
                // In the padded table_size buffer, they go at
                // [table_size-cutoff+1 .. table_size-1].
                for k in 1..cutoff {
                    padded[table_size - k] = spectrum[frame_size - k];
                }

                // DC bin (index 0) is already copied. Nyquist bin (index half)
                // is excluded for all levels since cutoff <= half.

                fft_inverse.process(&mut padded);

                let dest_base = (side * frame_count + frame_idx) * table_size;
                let dest = &mut level_table.samples[dest_base..dest_base + table_size];
                for (i, c) in padded.iter().enumerate() {
                    dest[i] = c.re * inv_n;
                }
            }
        }

        Self {
            levels,
            frame_size,
            table_size,
            frame_count,
            sides,
        }
    }

    /// Whether there is anything to read: at least one frame of nonzero size.
    pub fn is_playable(&self) -> bool {
        self.frame_count > 0 && self.frame_size > 0
    }

    /// Read an interpolated sample of `side` from mipmap level `level`.
    ///
    /// - `side` is clamped to the sides stored, so a one-sided table reads
    ///   the same on both.
    /// - `phase` is wrapped modulo 1.0 and linearly interpolated within a
    ///   frame.
    /// - `frame` is clamped to `[0, frame_count - 1]` and linearly
    ///   crossfaded between adjacent frames.
    ///
    #[inline]
    fn read_sample(&self, level: usize, side: usize, frame: f32, phase: f32) -> f32 {
        if self.frame_count == 0 {
            return 0.0;
        }

        let buf = &self.levels[level].samples;

        // Frame selection — clamp to [0, frame_count - 1].
        let max_frame_f = (self.frame_count - 1) as f32;
        let frame = frame.clamp(0.0, max_frame_f);
        let f0 = frame.floor();
        let f0_idx = f0 as usize;
        let f1_idx = (f0_idx + 1).min(self.frame_count - 1);
        let frame_frac = frame - f0;

        // Phase wrap to [0, 1).
        let phase = phase - phase.floor();
        let idx = phase * self.table_size as f32;
        let i0 = idx as usize;
        let i0 = if i0 >= self.table_size { 0 } else { i0 };
        let i1 = if i0 + 1 >= self.table_size { 0 } else { i0 + 1 };
        let phase_frac = idx - (i0 as f32);

        let side_base = side.min(self.sides - 1) * self.frame_count;
        let base0 = (side_base + f0_idx) * self.table_size;
        let base1 = (side_base + f1_idx) * self.table_size;

        let s0a = buf[base0 + i0];
        let s0b = buf[base0 + i1];
        let s0 = s0a + (s0b - s0a) * phase_frac;

        let s1a = buf[base1 + i0];
        let s1b = buf[base1 + i1];
        let s1 = s1a + (s1b - s1a) * phase_frac;

        s0 + (s1 - s0) * frame_frac
    }

    /// Read `side` at `mip`: level `mip.level`, blended toward the next level, or
    /// toward silence past the last, by `mip.next_weight`.
    ///
    /// Allocation-free; safe to call from the audio thread.
    #[inline]
    pub fn read(&self, mip: MipmapRead, side: usize, frame: f32, phase: f32) -> f32 {
        let sample = self.read_sample(mip.level, side, frame, phase);
        if mip.next_weight == 0.0 {
            return sample;
        }
        let next = if mip.level + 1 < self.levels.len() {
            self.read_sample(mip.level + 1, side, frame, phase)
        } else {
            0.0
        };
        sample + (next - sample) * mip.next_weight
    }

    /// The levels to read at playback frequency `freq` in Hz: the brightest
    /// level whose harmonics all stay below Nyquist, fading over the last
    /// [`FADE_OCTAVES`] below its limit into the next. Only the magnitude
    /// matters, since through-zero FM plays backwards with the same spectrum.
    ///
    /// Allocation-free; safe to call from the audio thread.
    #[inline]
    pub fn mipmap_for_freq(&self, freq: f32) -> MipmapRead {
        if freq.is_nan() {
            return MipmapRead::default();
        }
        let log_freq = freq.abs().log2();
        let level = self
            .levels
            .iter()
            .position(|level| log_freq < level.limit)
            .unwrap_or(self.levels.len() - 1);
        let fade_start = self.levels[level].limit - FADE_OCTAVES;
        MipmapRead {
            level,
            next_weight: ((log_freq - fade_start) / FADE_OCTAVES).clamp(0.0, 1.0),
        }
    }
}

/// How many spectrum bins level `level` keeps: half the frame at level 0,
/// halving at each level after. The kept bins are harmonics `0..cutoff`.
#[inline]
fn level_cutoff(frame_size: usize, level: usize) -> usize {
    frame_size / (2usize << level)
}

/// Floor(log2(n)) for n >= 1. Returns 0 for n == 0.
#[inline]
fn integer_log2(n: usize) -> usize {
    if n <= 1 {
        return 0;
    }
    (usize::BITS - 1 - n.leading_zeros()) as usize
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::SampleBuffer;
    use std::f32::consts::TAU;

    fn make_sine_frame(size: usize, phase_offset: f32) -> Vec<f32> {
        (0..size)
            .map(|i| (TAU * (i as f32 / size as f32) + phase_offset).sin())
            .collect()
    }

    fn make_cosine_frame(size: usize) -> Vec<f32> {
        (0..size)
            .map(|i| (TAU * (i as f32 / size as f32)).cos())
            .collect()
    }

    #[test]
    fn single_sine_frame_prepares_correctly() {
        let size = 2048;
        let samples = vec![make_sine_frame(size, 0.0)];
        let wav = WavData::new(SampleBuffer::from_samples(samples, 48000.0), Some(size));
        let prep = PreparedWavetable::from_wav_data(&wav, 48000.0);

        assert_eq!(prep.frame_count, 1);
        assert_eq!(prep.frame_size, size);
        // Bandwidths of 1023, 511, ... down to 1 harmonic: log2(2048) - 1 levels.
        assert_eq!(prep.levels.len(), 10);

        // Sine starts at 0.
        let s0 = prep.read_sample(0, 0, 0.0, 0.0);
        assert!(s0.abs() < 1e-4, "expected ~0 at phase 0, got {s0}");

        // Quarter-cycle of sine is +1.
        let s_quarter = prep.read_sample(0, 0, 0.0, 0.25);
        assert!(
            (s_quarter - 1.0).abs() < 1e-3,
            "expected ~1.0 at phase 0.25, got {s_quarter}"
        );

        // Sine fundamental is bin 1, far below any mipmap cutoff, so level 1
        // must be nearly identical to level 0.
        let s0_l1 = prep.read_sample(1, 0, 0.0, 0.0);
        let s_quarter_l1 = prep.read_sample(1, 0, 0.0, 0.25);
        assert!(s0_l1.abs() < 1e-3, "level1 phase 0: {s0_l1}");
        assert!(
            (s_quarter_l1 - 1.0).abs() < 1e-3,
            "level1 phase 0.25: {s_quarter_l1}"
        );
    }

    #[test]
    fn multi_frame_crossfades_between_frames() {
        let size = 2048;
        let mut channel: Vec<f32> = Vec::with_capacity(size * 2);
        channel.extend(make_sine_frame(size, 0.0));
        channel.extend(make_cosine_frame(size));
        let wav = WavData::new(
            SampleBuffer::from_samples(vec![channel], 48000.0),
            Some(size),
        );
        let prep = PreparedWavetable::from_wav_data(&wav, 48000.0);

        assert_eq!(prep.frame_count, 2);

        // At phase 0.25: sine = +1, cosine = 0. Midpoint ~= 0.5.
        let mid = prep.read_sample(0, 0, 0.5, 0.25);
        assert!((mid - 0.5).abs() < 5e-3, "expected ~0.5 midway, got {mid}");

        // At frame=0, phase=0.25 should still be ~1.0 (sine).
        let f0 = prep.read_sample(0, 0, 0.0, 0.25);
        assert!((f0 - 1.0).abs() < 1e-3, "frame 0 phase 0.25: {f0}");

        // At frame=1, phase=0.0 should be ~1.0 (cosine at 0).
        let f1 = prep.read_sample(0, 0, 1.0, 0.0);
        assert!((f1 - 1.0).abs() < 1e-3, "frame 1 phase 0: {f1}");
    }

    #[test]
    fn mipmap_attenuates_high_frequency_content() {
        let size = 2048;
        // Put a cosine at bin (size/2 - 2) — high, above the level-1 cutoff
        // of size/4 = 512.
        let bin = size / 2 - 2;
        let freq_norm = bin as f32 / size as f32;
        let channel: Vec<f32> = (0..size)
            .map(|i| (TAU * freq_norm * i as f32).cos())
            .collect();
        let wav = WavData::new(
            SampleBuffer::from_samples(vec![channel], 48000.0),
            Some(size),
        );
        let prep = PreparedWavetable::from_wav_data(&wav, 48000.0);

        let rms = |level: usize| -> f32 {
            let buf = &prep.levels[level].samples;
            let sum_sq: f32 = buf.iter().map(|x| x * x).sum();
            (sum_sq / buf.len() as f32).sqrt()
        };

        let rms0 = rms(0);
        let rms1 = rms(1);
        assert!(rms0 > 0.1, "level 0 RMS too low: {rms0}");
        assert!(
            rms1 < rms0 * 0.01,
            "expected level 1 to zero out the high bin; rms0={rms0} rms1={rms1}"
        );
    }

    fn prepared(size: usize, sample_rate: f32) -> PreparedWavetable {
        let wav = WavData::new(
            SampleBuffer::from_samples(vec![make_sine_frame(size, 0.0)], sample_rate),
            Some(size),
        );
        PreparedWavetable::from_wav_data(&wav, sample_rate)
    }

    #[test]
    fn mipmap_for_freq_picks_correct_level() {
        // A 2048-sample frame at 48 kHz: level `l` keeps harmonics up to
        // 1024 / 2^l - 1, so it plays alias-free below 24000 / (1024 / 2^l - 1) Hz.
        let prep = prepared(2048, 48000.0);
        let level = |freq: f32| prep.mipmap_for_freq(freq).level;
        assert_eq!(level(10.0), 0);
        assert_eq!(level(46.0), 1);
        assert_eq!(level(100.0), 3);
        assert_eq!(level(200.0), 4);
        assert_eq!(level(10000.0), 9);
        // Level 9, the last, keeps the fundamental alone, so it plays up to
        // Nyquist and is silent past it.
        assert_eq!(level(15000.0), 9);
        assert_eq!(level(23999.0), 9);
        for freq in [30000.0, f32::INFINITY] {
            let mip = prep.mipmap_for_freq(freq);
            assert_eq!(
                mip,
                MipmapRead {
                    level: 9,
                    next_weight: 1.0
                }
            );
            assert_eq!(prep.read(mip, 0, 0.0, 0.25), 0.0);
        }
        assert_eq!(prep.mipmap_for_freq(f32::NAN), MipmapRead::default());
        assert_eq!(prep.mipmap_for_freq(0.0), MipmapRead::default());
    }

    /// Every level keeps the fundamental, and every level read has all its
    /// harmonics below Nyquist, whatever the frame size.
    #[test]
    fn levels_read_are_alias_free_and_keep_the_fundamental() {
        let sample_rate = 48000.0;
        let nyquist = sample_rate / 2.0;
        for size in [2048, 600, 256, 4] {
            let prep = prepared(size, sample_rate);
            for level in 0..prep.levels.len() {
                assert!(
                    level_cutoff(size, level) >= 2,
                    "{size}-sample frame: level {level} drops the fundamental"
                );
            }
            let mut freq = 1.0f32;
            while freq < nyquist {
                let mip = prep.mipmap_for_freq(freq);
                let next = (mip.next_weight > 0.0 && mip.level + 1 < prep.levels.len())
                    .then_some(mip.level + 1);
                for level in std::iter::once(mip.level).chain(next) {
                    let top = (level_cutoff(size, level) - 1) as f32 * freq;
                    assert!(
                        top < nyquist,
                        "{size}-sample frame at {freq} Hz reads level {level}, whose top harmonic lands at {top} Hz"
                    );
                }
                freq *= 1.005;
            }
        }
    }

    /// The blend is continuous in frequency: it reaches the next level
    /// exactly where that level takes over, and is untouched below the fade.
    #[test]
    fn blend_is_continuous_across_levels() {
        let prep = prepared(2048, 48000.0);
        for level in 0..prep.levels.len() - 1 {
            let limit = prep.levels[level].limit;
            let below = prep.mipmap_for_freq(f32::exp2(limit - 1e-4));
            assert_eq!(below.level, level);
            assert!(
                below.next_weight > 0.999,
                "{below:?} just below level {level}'s limit"
            );
            let above = prep.mipmap_for_freq(f32::exp2(limit + 1e-4));
            assert_eq!(above.level, level + 1);
            assert!(
                above.next_weight < 1e-3,
                "{above:?} just above level {level}'s limit"
            );
            let before_fade = prep.mipmap_for_freq(f32::exp2(limit - FADE_OCTAVES - 1e-3));
            assert_eq!(
                before_fade,
                MipmapRead {
                    level,
                    next_weight: 0.0
                }
            );
        }
    }

    #[test]
    fn mipmap_uses_frequency_magnitude() {
        let prep = prepared(2048, 48000.0);
        // Backwards (negative-frequency) playback from through-zero FM has
        // the same magnitude spectrum, so it must read the same band-limited
        // levels as forward playback at the same speed.
        for f in [100.0, 200.0, 5000.0, 10000.0, 96000.0] {
            assert_eq!(
                prep.mipmap_for_freq(-f),
                prep.mipmap_for_freq(f),
                "levels for -{f} Hz must match +{f} Hz"
            );
        }
        assert!(prep.mipmap_for_freq(-5000.0).level > 0);
    }

    #[test]
    fn integer_log2_matches_expected() {
        assert_eq!(integer_log2(0), 0);
        assert_eq!(integer_log2(1), 0);
        assert_eq!(integer_log2(2), 1);
        assert_eq!(integer_log2(3), 1);
        assert_eq!(integer_log2(4), 2);
        assert_eq!(integer_log2(2048), 11);
        assert_eq!(integer_log2(4096), 12);
    }

    #[test]
    fn fallback_frame_size_when_metadata_missing_or_too_short() {
        let size = 2048;
        let channel = make_sine_frame(size, 0.0);
        for frame_size in [None, Some(3)] {
            let wav = WavData::new(
                SampleBuffer::from_samples(vec![channel.clone()], 48000.0),
                frame_size,
            );
            let prep = PreparedWavetable::from_wav_data(&wav, 48000.0);
            assert_eq!(prep.frame_size, DEFAULT_FRAME_SIZE);
        }
    }

    #[test]
    fn sub_frame_size_wav_produces_silent_table() {
        // Wav with fewer samples than fallback frame_size — should not panic,
        // should be silent.
        let samples = vec![vec![0.5f32; 100]]; // single channel, 100 samples, frame_size=2048 fallback
        let wav = WavData::new(SampleBuffer::from_samples(samples, 48000.0), None);
        let prep = PreparedWavetable::from_wav_data(&wav, 48000.0);
        assert_eq!(prep.frame_count, 0);
        // read_sample should return silence without panicking.
        assert_eq!(prep.read_sample(0, 0, 0.0, 0.0), 0.0);
        assert_eq!(prep.read_sample(5, 0, 0.5, 0.5), 0.0);
    }
}
