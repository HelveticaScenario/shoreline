#!/usr/bin/env python3
"""Writes the built-in Mutable Instruments wavetables to assets/builtin/tables/mi.

The waves are the union of the wavetables of Plaits and Beads, following
plaits/resources/wavetables.py and beads/resources/wavetables.py from Mutable
Instruments' eurorack repository (MIT, Emilie Gillet). Each family of waves is
one table, and the waves are computed at SIZE samples per frame; the ones taken
from Braids' 128-sample waves.bin are resampled through their spectrum. Every
frame is centred and normalized to a peak of 1, and each table is a 32-bit
float WAV whose Serum `clm ` chunk gives the frame size.

Usage: python3 -I scripts/builtin-assets/mi_wavetables.py <eurorack checkout>
"""

import pathlib
import struct
import sys

import numpy

SIZE = 2048
SAMPLE_RATE = 48000
OUT_DIR = (
    pathlib.Path(__file__).resolve().parents[2]
    / 'assets' / 'builtin' / 'tables' / 'mi'
)


def phase():
    return numpy.arange(0, SIZE) / float(SIZE)


def sine(frequency):
    if frequency >= SIZE / 2:
        return phase() * 0
    return numpy.sin(2 * numpy.pi * phase() * frequency)


def comb(n):
    return sum(sine(i + 1) for i in range(n))


def pair(n):
    return sum(
        sine(i + 1) * (i + 0.5) / (n - 1.0)
        + sine((i + 1) * 4) * (i + 0.5) / (n - 1.0) * 0.5
        for i in range(n)
    )


def tri(n, f=1):
    return sum(sine((2 * i + 1) * f) / (2 * i + 1) ** 2.0 for i in range(n))


def tri_stack(n):
    return sum(tri(15 + 5 * n, i + n // 3) for i in range(n))


def saw(n, f=1):
    return sum(sine((i + 1) * f) / (i + 1) for i in range(n))


def quadra(n):
    return sum(
        sine(2 * n + 2 * harmonic + 1) * amplitude
        for harmonic, amplitude in zip(range(4), [1, 0.5, 1, 0.5])
    )


def drawbars(bars):
    pipes = [1.0, 3.0, 2.0, 4.0, 6.0, 8.0, 10.0, 12.0, 16.0]
    return sum(
        int(intensity) / 8.0 * sine(frequency)
        for intensity, frequency in zip(bars, pipes)
    )


def pulse(duty):
    t = phase()
    t[-1] = t[0]
    return -numpy.where(t < duty, -1.0, 1.0)


def burst(duty):
    t = phase()
    t[-1] = t[0]
    return -numpy.where(t < duty ** 0.5, -1.0, 0.0) * sine(1.0 / duty)


def trisaw(harmonic):
    return tri(80) + saw(80, harmonic) * (1 if harmonic != 1 else 0.25) * 0.5


def sawtri(harmonic):
    return saw(80) * 0.5 + tri(80, harmonic) * (1 if harmonic != 1 else 0.25)


def bandpass_formant(ratio):
    return sine(ratio * 1.5) * (1.0 - phase()) * 0.5


def sine_power(power):
    x = sine(1.0) + saw(16)
    return numpy.sign(x) * (numpy.abs(x) ** (2.0 ** power))


def distort(x):
    return numpy.arctan(x * 8.0) / numpy.pi


def formant(index, shape, ratios, gains):
    t = phase()
    formant_1 = 3.9 * (index + 1) / 8.0
    formant_2 = formant_1 * (1.0 - numpy.cos(formant_1 * numpy.pi * 0.8))
    amplitude_1 = (1.0 - t) ** 0.2 * numpy.exp(-4.0 * t)
    amplitude_2 = (1.0 - t) ** 0.2 * numpy.exp(-2.0 * t)
    f = (
        shape(sine(1 + ratios[0] * formant_1)) * amplitude_1 * gains[0]
        + shape(sine(1 + ratios[1] * formant_2)) * amplitude_2 * gains[1]
        + shape(sine(1 + ratios[2] * (formant_2 + formant_1)))
        * amplitude_2 * gains[2]
    )
    return f - (f.max() + f.min()) / 2.0


def formant_f(index):
    return formant(index, lambda x: x, (3, 4, 2.8), (1, 1.5, 1.7))


def digi_formant_f(index):
    return formant(index, distort, (3.2, 4.1, 2.9), (1, 0.7, 0.7))


def fm(carrier, modulator, amount):
    return numpy.sin(
        2 * numpy.pi * (phase() * carrier + sine(modulator) * amount))


def braids(braids_waves, indices, keep_phase=False):
    """Braids waves resampled to SIZE. Unless `keep_phase`, every partial is
    set to the same phase, keeping the spectrum but not the shape."""
    family = []
    for i in indices:
        spectrum = numpy.fft.rfft(braids_waves[i * 129:i * 129 + 128] - 128.0)
        if not keep_phase:
            spectrum = numpy.abs(spectrum) * numpy.exp(-1j * numpy.pi / 2.0)
        family.append(numpy.fft.irfft(spectrum, SIZE))
    return family


def tables(braids_waves):
    """Every table, by file name, as its frames in scan order."""
    durations = [0.5, 0.4, 1 / 3.0, 0.25, 0.2, 0.125, 1 / 16.0, 1 / 32.0]
    sweep = numpy.linspace(0, 1.0, 16)
    return {
        'harmonics': [sine(n) for n in range(1, 17)],
        'harmonics-wide': [sine(n) for n in [2, 3, 4, 6, 8, 12, 16, 24]],
        'quadra': [quadra(n) for n in [2, 3, 4, 6, 8, 12, 16, 24]],
        'comb': [comb(n) for n in [2, 3, 5, 8, 13, 21, 34, 55]],
        'pair': [pair(n) for n in [2, 4, 6, 8, 10, 12, 14, 16]],
        'triangles': [tri_stack(n) for n in range(2, 18)],
        'organ': [drawbars(bars) for bars in [
            '688600000', '686040000', '666806000', '655550600',
            '665560060', '688500888', '660000888', '060000046',
            '867000006', '888876788', '668744354', '448644054',
            '327645222', '204675300', '002478500', '002050321']],
        'tri-saw': [trisaw(h) for h in [1, 1.5, 2, 3, 4, 4.5, 5, 8]]
        + [sawtri(h) for h in [8, 5, 4.5, 4, 3, 2, 1.5, 1]],
        'pulse': [pulse(d) for d in durations],
        'bursts': [burst(d) for d in durations],
        'band-pass': [bandpass_formant(r)
                      for r in [2.0, 3.0, 4.0, 6.0, 8.0, 9.0, 10.0, 16.0]],
        'sine-power': [sine_power(p) for p in range(8)],
        'formants': [formant_f(i) for i in range(8)],
        'digital-formants': [digi_formant_f(i) for i in range(8)],
        'male': braids(braids_waves, [0, 2, 4, 6, 8, 10, 12, 14]),
        'choir': braids(braids_waves, [32, 34, 36, 38, 40, 42, 44, 46]),
        'digital': braids(braids_waves, [176, 189, 191, 193, 195, 197, 199, 201]),
        'drone': braids(braids_waves, [203, 204, 205, 206, 207, 208, 209, 211]),
        'metal': braids(braids_waves, [220, 222, 224, 226, 228, 230, 232, 234]),
        'fantasy': braids(braids_waves, [236, 238, 240, 242, 244, 246, 248, 250]),
        'braids': braids(braids_waves, range(172, 188), keep_phase=True),
        'fm-octave': [fm(2.0, 1.0, x) for x in 4 * sweep ** 2],
        'fm-sub': [fm(1.0, 0.5, x) for x in 8 * sweep ** 2],
        'fm-bell': [fm(1.0, 3.0, x) for x in 3 * sweep ** 2],
        'fm-bright': [fm(1.0, 8.0, x) for x in 2 * sweep ** 2],
    }


def normalized(wave):
    x = numpy.asarray(wave, dtype=numpy.float64)
    x = x - x.mean()
    return x / numpy.abs(x).max()


def chunk(chunk_id, payload):
    padding = b'\0' if len(payload) % 2 else b''
    return chunk_id + struct.pack('<I', len(payload)) + payload + padding


def wav_bytes(frames):
    samples = numpy.concatenate([normalized(f) for f in frames])
    fmt = struct.pack('<HHIIHH', 3, 1, SAMPLE_RATE, SAMPLE_RATE * 4, 4, 32)
    body = (
        b'WAVE'
        + chunk(b'fmt ', fmt)
        + chunk(b'clm ', f'<!>{SIZE} 00000000 wavetable MI'.encode())
        + chunk(b'fact', struct.pack('<I', len(samples)))
        + chunk(b'data', samples.astype('<f4').tobytes())
    )
    return b'RIFF' + struct.pack('<I', len(body)) + body


def main():
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    waves_bin = pathlib.Path(sys.argv[1]) / 'plaits' / 'resources' / 'waves.bin'
    braids_waves = numpy.frombuffer(waves_bin.read_bytes(), numpy.uint8)
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    for name, frames in tables(braids_waves).items():
        assert all(len(f) == SIZE for f in frames)
        (OUT_DIR / f'{name}.wav').write_bytes(wav_bytes(frames))
        print(f'{name}.wav: {len(frames)} frames')


if __name__ == '__main__':
    main()
