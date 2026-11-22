//! Chord symbols written after a note and a colon (`c3:m7`, `g:7b9b13sus^1%`).
//!
//! A symbol is a quality, an extension, then any number of modifiers:
//! - quality: `m` minor, `dim` diminished, `h` half-diminished (a seventh
//!   chord), `aug` augmented, or nothing for major; `M` after it makes the
//!   seventh major, and `M` alone is the major triad.
//! - extension: `5` (root and fifth), `6`, `69`, `7`, `9`, `11` or `13`.
//!   `9`, `11` and `13` bring the extensions under them too (`9` has the
//!   7th); `11` leaves out the third of a major or dominant chord and `13` its
//!   eleventh, which clash.
//! - modifiers: `sus`/`sus4`, `sus2`; `addN` for N in 2, 3, 4, 6, 9, 11, 13;
//!   `b5`, `#5` (replace the fifth); `b6`, `b9`, `#9`, `#11`, `b13` (add that
//!   note, replacing its natural extension); `alt` (an altered dominant).
//!   Each names notes the others don't, so repeating one, changing the third
//!   or fifth twice, or altering an `alt` chord again is an error.
//! - then `^N` raises the lowest N notes an octave, and a final `%` tunes the
//!   chord in just intonation.

use arrayvec::ArrayVec;

use crate::poly::PORT_MAX_LANES;

/// A parsed chord symbol: semitones above the root, ascending, with the number
/// of its lowest notes to raise an octave.
pub struct ChordSymbol {
    pub voicing: ArrayVec<i8, PORT_MAX_LANES>,
    pub inversion: usize,
    pub just: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum Quality {
    Major,
    Minor,
    Diminished,
    HalfDiminished,
    Augmented,
}

/// Parse a chord symbol (see the module docs for the grammar).
pub fn parse(symbol: &str) -> Result<ChordSymbol, String> {
    let unknown = |rest: &str| {
        if symbol.starts_with("maj") || symbol.starts_with("min") {
            format!(
                "unknown chord \"{symbol}\": write `M` for major and `m` for minor, as in M7 or m7"
            )
        } else {
            format!("unknown chord \"{symbol}\" at \"{rest}\"")
        }
    };
    let (body, just) = match symbol.strip_suffix('%') {
        Some(body) => (body, true),
        None => (symbol, false),
    };
    let (body, inversion) = match body.split_once('^') {
        Some((body, n)) => (
            body,
            n.parse::<usize>().map_err(|_| {
                format!("invalid inversion \"^{n}\" in \"{symbol}\" (expected ^1, ^2, …)")
            })?,
        ),
        None => (body, 0),
    };
    if body.is_empty() {
        return Err(format!("chord \"{symbol}\" names no quality"));
    }

    let mut rest = body;
    let mut eat = |prefix: &str| match rest.strip_prefix(prefix) {
        Some(after) => {
            rest = after;
            true
        }
        None => false,
    };

    let quality = if eat("dim") {
        Quality::Diminished
    } else if eat("aug") {
        Quality::Augmented
    } else if eat("h") {
        Quality::HalfDiminished
    } else if eat("m") {
        Quality::Minor
    } else {
        Quality::Major
    };
    let major_seventh = eat("M");
    let (digits, after) = split_digits(rest);
    if quality == Quality::HalfDiminished
        && (major_seventh
            || !matches!(digits, "" | "7" | "9" | "11" | "13")
            || ["m", "dim", "aug", "h"]
                .iter()
                .any(|q| after.starts_with(q)))
    {
        return Err(format!(
            "\"{symbol}\": `h` is a half-diminished seventh, so it takes 7, 9, 11 or 13"
        ));
    }
    let extension: u8 = match digits {
        "" => 0,
        "5" => 5,
        "6" => 6,
        "69" => 69,
        "7" => 7,
        "9" => 9,
        "11" => 11,
        "13" => 13,
        _ => return Err(unknown(rest)),
    };
    rest = after;
    if major_seventh && quality != Quality::Major && !matches!(extension, 7 | 9 | 11 | 13) {
        return Err(format!(
            "\"{symbol}\": `M` after a quality makes its seventh major, so it needs 7, 9, 11 or 13"
        ));
    }

    let minor = matches!(quality, Quality::Minor | Quality::HalfDiminished);
    let stacks = |n: u8| extension >= n && extension != 69;
    let mut third = match quality {
        _ if extension == 5 || (extension == 11 && !minor) => None,
        Quality::Major | Quality::Augmented => Some(4),
        _ => Some(3),
    };
    let mut fifth = Some(match quality {
        Quality::Diminished | Quality::HalfDiminished => 6,
        Quality::Augmented => 8,
        _ => 7,
    });
    let mut seventh = match (quality, extension) {
        (_, 6 | 69) => Some(9),
        (Quality::HalfDiminished, _) => Some(10),
        (_, 0 | 5) => None,
        _ if major_seventh => Some(11),
        (Quality::Diminished, _) => Some(9),
        _ => Some(10),
    };
    let mut ninth = (stacks(9) || extension == 69).then_some(14);
    let mut eleventh = (stacks(11) && (minor || extension == 11)).then_some(17);
    let mut thirteenth = (extension == 13).then_some(21);
    // The modifier that set each part, for errors that name it.
    let mut third_by: Option<&str> = None;
    let mut fifth_by: Option<&str> = None;
    let mut altered_by: Option<&str> = None;
    let mut added_by: [Option<&str>; 32] = [None; 32];
    let already = |what: &str| format!("\"{symbol}\": {what}");
    let twice = |token: &str| already(&format!("`{token}` is written twice"));
    let alt_takes_no = |token: &str| {
        already(&format!(
            "`alt` alters the 5th, 9th, 11th and 13th itself, so it doesn't take `{token}`"
        ))
    };

    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix("sus") {
            let (interval, name, after) = match after.as_bytes().first() {
                Some(b'2') => (2, "2nd", &after[1..]),
                Some(b'4') => (5, "4th", &after[1..]),
                _ => (5, "4th", after),
            };
            let token = &rest[..rest.len() - after.len()];
            match third_by {
                Some(by) if by == token => return Err(twice(token)),
                Some(by) => {
                    return Err(already(&format!("the third is already replaced by `{by}`")));
                }
                None => {}
            }
            if let Some(by) = added_by[interval as usize] {
                return Err(already(&format!(
                    "`{token}` already has the {name}, so `{by}` adds nothing"
                )));
            }
            third_by = Some(token);
            third = Some(interval);
            rest = after;
            continue;
        }
        if let Some(after) = rest.strip_prefix("alt") {
            match altered_by {
                Some("alt") => return Err(twice("alt")),
                Some(by) => return Err(alt_takes_no(by)),
                None => {}
            }
            altered_by = Some("alt");
            fifth = None;
            seventh = seventh.or(Some(10));
            ninth = None;
            eleventh = None;
            thirteenth = None;
            for interval in [13, 15, 18, 20] {
                added_by[interval] = Some("alt");
            }
            rest = after;
            continue;
        }
        let (kind, after) = if let Some(after) = rest.strip_prefix("add") {
            ("add", after)
        } else if let Some(after) = rest.strip_prefix('b') {
            ("b", after)
        } else if let Some(after) = rest.strip_prefix('#') {
            ("#", after)
        } else {
            return Err(unknown(rest));
        };
        let (digits, after) = split_digits(after);
        let token = &rest[..rest.len() - after.len()];
        let fifth_change = match (kind, digits) {
            ("b", "5") => Some(6),
            ("#", "5") => Some(8),
            _ => None,
        };
        let note = match (kind, digits) {
            ("add", "2") => Some((2, "2nd")),
            ("add", "3") => Some((4, "3rd")),
            ("add", "4") => Some((5, "4th")),
            ("add", "6") => Some((9, "6th")),
            ("add", "9") => Some((14, "9th")),
            ("add", "11") => Some((17, "11th")),
            ("add", "13") => Some((21, "13th")),
            ("b", "6") => Some((8, "♭6th")),
            ("b", "9") => Some((13, "♭9th")),
            ("#", "9") => Some((15, "♯9th")),
            ("#", "11") => Some((18, "♯11th")),
            ("b", "13") => Some((20, "♭13th")),
            _ => None,
        };
        if fifth_change.is_none() && note.is_none() {
            return Err(unknown(rest));
        }
        if kind != "add" && token != "b6" {
            if altered_by == Some("alt") {
                return Err(alt_takes_no(token));
            }
            altered_by = altered_by.or(Some(token));
        }
        if let Some(interval) = fifth_change {
            match fifth_by {
                Some(by) if by == token => return Err(twice(token)),
                Some(by) => return Err(already(&format!("the 5th is already changed by `{by}`"))),
                None => {}
            }
            if let Some(by) = added_by[interval as usize] {
                return Err(already(&format!(
                    "`{token}` already has that note, so `{by}` adds nothing"
                )));
            }
            fifth_by = Some(token);
            fifth = Some(interval);
        }
        if let Some((interval, name)) = note {
            if added_by[interval as usize] == Some(token) {
                return Err(twice(token));
            }
            let present = added_by[interval as usize].is_some()
                || [third, fifth, seventh, ninth, eleventh, thirteenth].contains(&Some(interval));
            if present {
                return Err(already(&format!(
                    "the chord already has the {name}, so `{token}` adds nothing"
                )));
            }
            added_by[interval as usize] = Some(token);
            match interval {
                13 | 15 => ninth = None,
                18 => eleventh = None,
                20 => thirteenth = None,
                _ => {}
            }
        }
        rest = after;
    }

    let base = [Some(0), third, fifth, seventh, ninth, eleventh, thirteenth];
    let voicing = (0..32i8)
        .filter(|&i| added_by[i as usize].is_some() || base.contains(&Some(i)))
        .collect();
    Ok(ChordSymbol {
        voicing,
        inversion,
        just,
    })
}

/// Split the leading run of ASCII digits off `s`.
fn split_digits(s: &str) -> (&str, &str) {
    s.split_at(s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notes(symbol: &str) -> Vec<i8> {
        parse(symbol)
            .unwrap_or_else(|e| panic!("{symbol}: {e}"))
            .voicing
            .to_vec()
    }

    /// iReal Pro's chord qualities, in this notation.
    #[test]
    fn ireal_pro_qualities() {
        let cases: &[(&str, &[i8])] = &[
            ("M", &[0, 4, 7]),
            ("m", &[0, 3, 7]),
            ("m6", &[0, 3, 7, 9]),
            ("m7", &[0, 3, 7, 10]),
            ("m9", &[0, 3, 7, 10, 14]),
            ("m11", &[0, 3, 7, 10, 14, 17]),
            ("m13", &[0, 3, 7, 10, 14, 17, 21]),
            ("m#5", &[0, 3, 8]),
            ("mM7", &[0, 3, 7, 11]),
            ("madd2", &[0, 2, 3, 7]),
            ("madd9", &[0, 3, 7, 14]),
            ("M7", &[0, 4, 7, 11]),
            ("M9", &[0, 4, 7, 11, 14]),
            ("M13", &[0, 4, 7, 11, 14, 21]),
            ("M7#5", &[0, 4, 8, 11]),
            ("M7b5", &[0, 4, 6, 11]),
            ("M7#11", &[0, 4, 7, 11, 18]),
            ("dim", &[0, 3, 6]),
            ("dim7", &[0, 3, 6, 9]),
            ("dimM7", &[0, 3, 6, 11]),
            ("h", &[0, 3, 6, 10]),
            ("h7", &[0, 3, 6, 10]),
            ("m7b5", &[0, 3, 6, 10]),
            ("aug", &[0, 4, 8]),
            ("6", &[0, 4, 7, 9]),
            ("add2", &[0, 2, 4, 7]),
            ("add9", &[0, 4, 7, 14]),
            ("7", &[0, 4, 7, 10]),
            ("9", &[0, 4, 7, 10, 14]),
            ("11", &[0, 7, 10, 14, 17]),
            ("13", &[0, 4, 7, 10, 14, 21]),
            ("7#5", &[0, 4, 8, 10]),
            ("aug7", &[0, 4, 8, 10]),
            ("7b13", &[0, 4, 7, 10, 20]),
            ("7#11", &[0, 4, 7, 10, 18]),
            ("7b5", &[0, 4, 6, 10]),
            ("7alt", &[0, 4, 10, 13, 15, 18, 20]),
            ("alt", &[0, 4, 10, 13, 15, 18, 20]),
            ("7b9b13", &[0, 4, 7, 10, 13, 20]),
            ("7b9#11", &[0, 4, 7, 10, 13, 18]),
            ("7#9#11", &[0, 4, 7, 10, 15, 18]),
            ("9#5", &[0, 4, 8, 10, 14]),
            ("9#11", &[0, 4, 7, 10, 14, 18]),
            ("sus2", &[0, 2, 7]),
            ("sus4", &[0, 5, 7]),
            ("sus", &[0, 5, 7]),
            ("7sus", &[0, 5, 7, 10]),
            ("9sus", &[0, 5, 7, 10, 14]),
            ("7susb9", &[0, 5, 7, 10, 13]),
            ("7susb9b13", &[0, 5, 7, 10, 13, 20]),
            ("7b9b13sus", &[0, 5, 7, 10, 13, 20]),
            ("5", &[0, 7]),
            ("69", &[0, 4, 7, 9, 14]),
            ("m69", &[0, 3, 7, 9, 14]),
            ("mb6", &[0, 3, 7, 8]),
            ("mM9", &[0, 3, 7, 11, 14]),
            ("M9#11", &[0, 4, 7, 11, 14, 18]),
            ("h9", &[0, 3, 6, 10, 14]),
            ("7b9", &[0, 4, 7, 10, 13]),
            ("7#9", &[0, 4, 7, 10, 15]),
            ("9b5", &[0, 4, 6, 10, 14]),
            ("7#9#5", &[0, 4, 8, 10, 15]),
            ("7#9b5", &[0, 4, 6, 10, 15]),
            ("7b9b5", &[0, 4, 6, 10, 13]),
            ("7b9#5", &[0, 4, 8, 10, 13]),
            ("7b9#9", &[0, 4, 7, 10, 13, 15]),
            ("13#11", &[0, 4, 7, 10, 14, 18, 21]),
            ("13b9", &[0, 4, 7, 10, 13, 21]),
            ("13#9", &[0, 4, 7, 10, 15, 21]),
            ("13sus", &[0, 5, 7, 10, 14, 21]),
            ("7b13sus", &[0, 5, 7, 10, 20]),
            ("7susadd3", &[0, 4, 5, 7, 10]),
        ];
        for (symbol, expected) in cases {
            assert_eq!(notes(symbol), *expected, "{symbol}");
        }
    }

    #[test]
    fn inversion_and_just_intonation() {
        let inverted = parse("M7^2%").unwrap();
        assert_eq!(inverted.voicing.to_vec(), [0, 4, 7, 11]);
        assert_eq!(inverted.inversion, 2);
        assert!(inverted.just);
        assert!(!parse("M7").unwrap().just);
    }

    #[test]
    fn rejects_what_it_cannot_read() {
        for symbol in [
            "", "%", "maj7", "x", "8", "7b10", "add5", "mM", "h6", "hM7", "m^", "7^x",
        ] {
            assert!(parse(symbol).is_err(), "{symbol}");
        }
        assert!(parse("maj7").err().unwrap().contains("write `M` for major"));
        for symbol in ["h5", "h6", "h69", "h8", "hM7", "hm7", "hdim", "haug", "hh"] {
            let error = parse(symbol).err().unwrap();
            assert!(
                error.contains("`h` is a half-diminished seventh"),
                "{symbol}: {error}"
            );
        }
    }

    #[test]
    fn rejects_modifiers_that_repeat_or_contradict() {
        let cases = [
            ("altalt", "`alt` is written twice"),
            (
                "7altb9",
                "`alt` alters the 5th, 9th, 11th and 13th itself, so it doesn't take `b9`",
            ),
            (
                "7#5alt",
                "`alt` alters the 5th, 9th, 11th and 13th itself, so it doesn't take `#5`",
            ),
            ("7b5#5", "the 5th is already changed by `b5`"),
            ("7b5b5", "`b5` is written twice"),
            ("sus2sus4", "the third is already replaced by `sus2`"),
            ("sus4sus4", "`sus4` is written twice"),
            ("7b9b9", "`b9` is written twice"),
            ("add9add9", "`add9` is written twice"),
            (
                "9add9",
                "the chord already has the 9th, so `add9` adds nothing",
            ),
            (
                "sus2add2",
                "the chord already has the 2nd, so `add2` adds nothing",
            ),
            (
                "add2sus2",
                "`sus2` already has the 2nd, so `add2` adds nothing",
            ),
            (
                "7add4sus",
                "`sus` already has the 4th, so `add4` adds nothing",
            ),
            (
                "7#5b6",
                "the chord already has the ♭6th, so `b6` adds nothing",
            ),
            ("7b6#5", "`#5` already has that note, so `b6` adds nothing"),
        ];
        for (symbol, message) in cases {
            let error = parse(symbol).err().unwrap();
            assert_eq!(error, format!("\"{symbol}\": {message}"));
        }
        for symbol in ["7b9#9", "7#9#11", "sus2add4", "7susadd3", "7altb6"] {
            assert!(parse(symbol).is_ok(), "{symbol}");
        }
    }
}
