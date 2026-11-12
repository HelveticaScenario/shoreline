#!/usr/bin/env python3
"""Writes the Plaits lookup tables the `$mi.*` modules read as Rust statics.

Usage: generate_tables.py   (run fetch.sh first)
"""
import pathlib
import re

HERE = pathlib.Path(__file__).resolve().parent
SOURCE = HERE / ".src/eurorack"
RESOURCES = SOURCE / "plaits/resources.cc"
UNITS = SOURCE / "stmlib/dsp/units.cc"
DX_UNITS = SOURCE / "plaits/dsp/fm/dx_units.cc"
SPEECH = SOURCE / "plaits/dsp/speech"
NAIVE = SPEECH / "naive_speech_synth.cc"
SAM = SPEECH / "sam_speech_synth.cc"
LPC_PHONEMES = SPEECH / "lpc_speech_synth_phonemes.cc"
LPC_CONTROLLER = SPEECH / "lpc_speech_synth_controller.cc"
LPC_WORDS = SPEECH / "lpc_speech_synth_words.cc"
OUT = HERE.parents[1] / "crates/modular_core/src/dsp/mi/core/tables.rs"

# (C name, element type[, source file]) for every table a module reads; the
# source defaults to Plaits' resources.
TABLES = [
    ("lut_sine", "f32"),
    ("lut_fm_frequency_quantizer", "f32"),
    ("lut_4x_downsampler_fir", "f32"),
    ("lut_fold", "f32"),
    ("lut_fold_2", "f32"),
    ("lut_ws_inverse_tan", "i16"),
    ("lut_ws_inverse_sin", "i16"),
    ("lut_ws_linear", "i16"),
    ("lut_ws_bump", "i16"),
    ("lut_ws_double_bump", "i16"),
    ("lut_stiffness", "f32"),
    ("lut_svf_shift", "f32"),
    ("wav_integrated_waves", "i16"),
    ("lut_pitch_ratio_high", "f32", UNITS),
    ("lut_pitch_ratio_low", "f32", UNITS),
    ("syx_bank_0", "u8"),
    ("syx_bank_1", "u8"),
    ("syx_bank_2", "u8"),
    ("lut_coarse", "f32", DX_UNITS),
    ("lut_amp_mod_sensitivity", "f32", DX_UNITS),
    ("lut_pitch_mod_sensitivity", "f32", DX_UNITS),
    ("lut_cube_root", "f32", DX_UNITS),
    ("lut_lpc_excitation_pulse", "i8"),
    ("formant_amplitude_lut", "f32", SAM),
]

# (Rust name, element type, the C definition the table starts after, source
# file) for the speech synths' phoneme tables, which are arrays of structs:
# their fields flattened in declaration order.
STRUCT_TABLES = [
    ("naive_phonemes", "u8", "NaiveSpeechSynth::phonemes_[][kNaiveSpeechNumRegisters] =", NAIVE),
    ("sam_phonemes", "u8", "SAMSpeechSynth::phonemes_[] =", SAM),
    ("lpc_phonemes", "i16", "LPCSpeechSynthController::phonemes_[kLPCSpeechSynthNumPhonemes] =", LPC_PHONEMES),
]


def parse(source, name, path):
    match = re.search(
        r"(?:static )?const (?:float|int16_t|uint8_t|int8_t|uint16_t) (?:\w+::)?%s\[\d*\] = \{(.*?)\};" % name,
        source,
        re.S,
    )
    if match is None:
        raise SystemExit(f"{name} not found in {path}")
    values = [v.strip() for v in match.group(1).split(",") if v.strip()]
    return [v if v.startswith("0x") else v.removesuffix("f") for v in values]


def parse_structs(source, definition, path):
    start = source.find(definition)
    if start < 0:
        raise SystemExit(f"{definition} not found in {path}")
    body = source[start + len(definition) : source.index("};", start)]
    body = re.sub(r"//[^\n]*", "", body)
    return re.findall(r"-?\d+", body)


class BitStream:
    """Plaits' `BitStream`: bits read from the low end of each byte."""

    def __init__(self, data, at):
        self.data = data
        self.at = at
        self.available = 0
        self.bits = 0

    def get(self, count):
        shift = count
        if count > self.available:
            self.bits = (self.bits << self.available) & 0xFFFF
            shift -= self.available
            self.bits |= int(f"{self.data[self.at]:08b}"[::-1], 2)
            self.at += 1
            self.available += 8
        self.bits = (self.bits << shift) & 0xFFFF
        result = self.bits >> 8
        self.bits &= 0xFF
        self.available -= count
        return result

    def flush(self):
        while self.available:
            self.get(1)


def decode_words(controller, words):
    """Every word of Plaits' five LPC word banks decoded as
    `LPCSpeechSynthWordBank::LoadNextWord` decodes them: the frames' fields
    in declaration order, the first frame of each word (then the frame count)
    and the first word of each bank (then the word count)."""
    luts = {
        name: [int(v, 0) for v in parse(controller, f"LPCSpeechSynthWordBank::{name}_lut_", LPC_CONTROLLER)]
        for name in ["energy", "period", "k0", "k1", "k2", "k3", "k4", "k5", "k6", "k7", "k8", "k9"]
    }
    widths = [5, 5, 4, 4, 4, 4, 4, 3, 3, 3]
    frames, word_starts, bank_words = [], [], []
    for bank in range(5):
        data = [int(v, 0) for v in parse(re.sub(r"//[^\n]*", "", words), f"bank_{bank}", LPC_WORDS)]
        bank_words.append(len(word_starts))
        at = 0
        while at < len(data):
            word_starts.append(len(frames))
            stream = BitStream(data, at)
            frame = [0] * 12
            while True:
                energy = stream.get(4)
                if energy == 0xF:
                    stream.flush()
                    break
                frame[0] = luts["energy"][energy]
                if energy:
                    repeat = stream.get(1)
                    frame[1] = luts["period"][stream.get(6)]
                    if not repeat:
                        # Unvoiced frames code four coefficients and keep the rest.
                        for k in range(10 if frame[1] else 4):
                            frame[2 + k] = luts[f"k{k}"][stream.get(widths[k])]
                frames.append(list(frame))
            at = stream.at
    word_starts.append(len(frames))
    bank_words.append(len(word_starts) - 1)
    return [str(v) for f in frames for v in f], [str(v) for v in word_starts], [str(v) for v in bank_words]


def emit(lines, name, ty, values):
    lines.append("#[rustfmt::skip]")
    lines.append(f"pub static {name.upper()}: [{ty}; {len(values)}] = [")
    for i in range(0, len(values), 4):
        lines.append("    " + ", ".join(values[i : i + 4]) + ",")
    lines.append("];")
    lines.append("")


def main():
    sources = {}
    lines = [
        "//! Plaits and stmlib lookup tables, generated by",
        "//! `scripts/mi-reference/generate_tables.py` from `plaits/resources.cc`,",
        "//! `plaits/dsp/fm/dx_units.cc`, `plaits/dsp/speech/*.cc` (the LPC word banks",
        "//! decoded into frames) and `stmlib/dsp/units.cc` at the commits `fetch.sh`",
        "//! pins. Do not edit.",
        "",
        "#![allow(clippy::approx_constant, clippy::excessive_precision)]",
        "",
    ]
    for name, ty, *path in TABLES:
        path = path[0] if path else RESOURCES
        if path not in sources:
            sources[path] = path.read_text()
        emit(lines, name, ty, parse(sources[path], name, path))
    for name, ty, definition, path in STRUCT_TABLES:
        if path not in sources:
            sources[path] = path.read_text()
        emit(lines, name, ty, parse_structs(sources[path], definition, path))
    frames, word_starts, bank_words = decode_words(LPC_CONTROLLER.read_text(), LPC_WORDS.read_text())
    emit(lines, "lpc_word_frames", "i16", frames)
    emit(lines, "lpc_word_starts", "u16", word_starts)
    emit(lines, "lpc_bank_words", "u8", bank_words)
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text("\n".join(lines))


if __name__ == "__main__":
    main()
