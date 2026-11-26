//! DX7 voices read from a `.syx` file: 32-voice bulk dumps, single-voice
//! dumps, any number of either one after another, or a bare 4096-byte bank.

use super::patch::PACKED_SIZE;

/// One voice of a file: its name and its bytes in the packed format.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyxVoice {
    pub name: String,
    pub packed: [u8; PACKED_SIZE],
}

const BANK_VOICES: usize = 32;
const BANK_SIZE: usize = BANK_VOICES * PACKED_SIZE;
const SINGLE_SIZE: usize = 155;
const OPERATOR_SIZE: usize = 21;
const NAME: std::ops::Range<usize> = 118..128;

/// Every voice in `bytes`, in file order. Checksums are not checked, since
/// files in the wild often carry wrong ones; sysex messages from other
/// devices, or of other DX7 formats, are skipped.
pub fn parse_syx(bytes: &[u8]) -> Result<Vec<SyxVoice>, String> {
    if bytes.first() != Some(&0xf0) {
        return if bytes.len() == BANK_SIZE {
            Ok(bank_voices(bytes))
        } else {
            Err("not a DX7 sysex file: it does not start with a sysex message (F0) and is not a bare 4096-byte bank".into())
        };
    }
    let mut voices = Vec::new();
    let mut rest = bytes;
    while let Some(start) = rest.iter().position(|&b| b == 0xf0) {
        let message = &rest[start..];
        // Any status byte ends a sysex message, so one missing its F7 still
        // leaves the next F0 to start the next message.
        let end = message[1..]
            .iter()
            .position(|&b| b >= 0x80)
            .map_or(message.len(), |i| i + 1);
        voices.extend(message_voices(&message[..end]));
        rest = &message[end..];
    }
    if voices.is_empty() {
        return Err("the file holds no DX7 voice or bank dump".into());
    }
    Ok(voices)
}

/// The voices of one sysex message, from its F0 up to but not including its
/// F7: a Yamaha (43) dump of format 0 (one voice) or 9 (32 voices), whose
/// data follows a six-byte header.
fn message_voices(message: &[u8]) -> Vec<SyxVoice> {
    let [0xf0, 0x43, _, format, _, _, data @ ..] = message else {
        return Vec::new();
    };
    match format {
        0 if data.len() >= SINGLE_SIZE => vec![single_voice(&data[..SINGLE_SIZE])],
        9 if data.len() >= BANK_SIZE => bank_voices(&data[..BANK_SIZE]),
        _ => Vec::new(),
    }
}

fn bank_voices(bank: &[u8]) -> Vec<SyxVoice> {
    bank.as_chunks::<PACKED_SIZE>()
        .0
        .iter()
        .map(|&packed| SyxVoice {
            name: voice_name(&packed[NAME]),
            packed,
        })
        .collect()
}

/// A single-voice dump's 155 unpacked bytes in the packed layout.
fn single_voice(u: &[u8]) -> SyxVoice {
    let mut p = [0u8; PACKED_SIZE];
    for op in 0..6 {
        let s = &u[op * OPERATOR_SIZE..(op + 1) * OPERATOR_SIZE];
        let d = &mut p[op * 17..(op + 1) * 17];
        d[..11].copy_from_slice(&s[..11]);
        d[11] = (s[11] & 0x3) | ((s[12] & 0x3) << 2);
        d[12] = (s[13] & 0x7) | ((s[20] & 0xf) << 3);
        d[13] = (s[14] & 0x3) | ((s[15] & 0x7) << 2);
        d[14] = s[16];
        d[15] = (s[17] & 0x1) | ((s[18] & 0x1f) << 1);
        d[16] = s[19];
    }
    let g = &u[126..];
    p[102..110].copy_from_slice(&g[..8]);
    p[110] = g[8] & 0x1f;
    p[111] = (g[9] & 0x7) | ((g[10] & 0x1) << 3);
    p[112..116].copy_from_slice(&g[11..15]);
    p[116] = (g[15] & 0x1) | ((g[16] & 0x7) << 1) | ((g[17] & 0x7) << 4);
    p[117] = g[18];
    p[NAME].copy_from_slice(&g[19..29]);
    SyxVoice {
        name: voice_name(&p[NAME]),
        packed: p,
    }
}

/// A voice's ten name bytes as text: printable ASCII kept, anything else a
/// space, trailing spaces trimmed.
fn voice_name(bytes: &[u8]) -> String {
    let name: String = bytes
        .iter()
        .map(|&b| {
            if (0x20..0x7f).contains(&b) {
                b as char
            } else {
                ' '
            }
        })
        .collect();
    name.trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packed_voice(name: &[u8; 10], fill: u8) -> [u8; PACKED_SIZE] {
        let mut v = [fill; PACKED_SIZE];
        v[NAME].copy_from_slice(name);
        v
    }

    fn bulk_dump(voices: &[[u8; PACKED_SIZE]; BANK_VOICES]) -> Vec<u8> {
        let mut out = vec![0xf0, 0x43, 0x00, 0x09, 0x20, 0x00];
        let data: Vec<u8> = voices.iter().flatten().copied().collect();
        let checksum = data.iter().fold(0u8, |a, &b| a.wrapping_sub(b)) & 0x7f;
        out.extend(data);
        out.extend([checksum, 0xf7]);
        out
    }

    fn bank() -> [[u8; PACKED_SIZE]; BANK_VOICES] {
        std::array::from_fn(|i| {
            packed_voice(
                format!("VOICE {i:<4}").as_bytes().try_into().unwrap(),
                i as u8,
            )
        })
    }

    #[test]
    fn reads_a_bulk_dump() {
        let voices = parse_syx(&bulk_dump(&bank())).unwrap();
        assert_eq!(voices.len(), 32);
        assert_eq!(voices[0].name, "VOICE 0");
        assert_eq!(voices[31].name, "VOICE 31");
        assert_eq!(voices[5].packed, bank()[5]);
    }

    #[test]
    fn reads_a_bare_bank() {
        let raw: Vec<u8> = bank().iter().flatten().copied().collect();
        let voices = parse_syx(&raw).unwrap();
        assert_eq!(voices.len(), 32);
        assert_eq!(voices[7].packed, bank()[7]);
    }

    #[test]
    fn reads_consecutive_dumps_and_skips_other_messages() {
        let mut file = bulk_dump(&bank());
        file.extend([0xf0, 0x41, 0x10, 0x42, 0xf7]);
        file.extend(bulk_dump(&bank()));
        let voices = parse_syx(&file).unwrap();
        assert_eq!(voices.len(), 64);
        assert_eq!(voices[32].name, "VOICE 0");
    }

    #[test]
    fn reads_every_builtin_voice() {
        use crate::dsp::mi::six_op::test_voices::{PLAITS_VOICES, asset};
        for path in PLAITS_VOICES {
            let voices = parse_syx(&std::fs::read(asset(path)).unwrap()).unwrap();
            assert_eq!(voices.len(), 1, "{path}");
            assert!(!voices[0].name.is_empty(), "{path}");
        }
        let first = parse_syx(&std::fs::read(asset(PLAITS_VOICES[0])).unwrap()).unwrap();
        assert_eq!(first[0].name, "SOLID BASS");
    }

    #[test]
    fn a_dump_missing_its_f7_ends_at_the_next_message() {
        let mut first = bulk_dump(&bank());
        first.pop();
        let mut second = bank();
        second[0] = packed_voice(b"SECOND    ", 1);
        first.extend(bulk_dump(&second));
        let voices = parse_syx(&first).unwrap();
        assert_eq!(voices.len(), 64);
        assert_eq!(voices[31].name, "VOICE 31");
        assert_eq!(voices[32].name, "SECOND");
    }

    #[test]
    fn names_keep_inner_spaces_and_blank_unprintables() {
        let voices = parse_syx(&bulk_dump(&std::array::from_fn(|_| {
            packed_voice(b"BASS  \x013 \x00", 0)
        })))
        .unwrap();
        assert_eq!(voices[0].name, "BASS   3");
    }

    #[test]
    fn rejects_files_without_voices() {
        assert!(parse_syx(b"RIFF").is_err());
        assert!(parse_syx(&[0xf0, 0x41, 0x10, 0xf7]).is_err());
        assert!(parse_syx(&[0xf0, 0x43, 0x00, 0x09, 0x20, 0x00, 1, 2, 3, 0xf7]).is_err());
    }
}
