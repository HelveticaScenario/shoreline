//! The DX7's 32 algorithms (`fm/algorithms.h`), compiled as Plaits compiles
//! them into chains of operators that one render call runs together.

const DESTINATION_MASK: u8 = 0x03;
const SOURCE_MASK: u8 = 0x30;
const SOURCE_FEEDBACK: u8 = 0x30;
const ADDITIVE_FLAG: u8 = 0x04;
const FEEDBACK_SOURCE_FLAG: u8 = 0x40;

const fn modulation(buffer: u8) -> u8 {
    buffer << 4
}
const fn add(buffer: u8) -> u8 {
    buffer | ADDITIVE_FLAG
}
const FB_SRC: u8 = FEEDBACK_SOURCE_FLAG;
const FB_DST: u8 = modulation(3);
const FB: u8 = FB_SRC | FB_DST;
const NO_MOD: u8 = modulation(0);
const OUTPUT: u8 = add(0);

/// Each operator's opcode: where its modulation comes from (none, a buffer or
/// feedback), which buffer it writes, and whether it adds to it. Buffer 0 is
/// the voice's output. Rows list operators 6 down to 1, the order they
/// render in.
#[rustfmt::skip]
const OPCODES: [[u8; 6]; 32] = [
    // Algorithm 1
    [
        FB | 1,
        modulation(1) | 1,
        modulation(1) | 1,
        modulation(1) | OUTPUT,
        NO_MOD | 1,
        modulation(1) | add(0),
    ],
    // Algorithm 2
    [
        NO_MOD | 1,
        modulation(1) | 1,
        modulation(1) | 1,
        modulation(1) | OUTPUT,
        FB | 1,
        modulation(1) | add(0),
    ],
    // Algorithm 3
    [
        FB | 1,
        modulation(1) | 1,
        modulation(1) | OUTPUT,
        NO_MOD | 1,
        modulation(1) | 1,
        modulation(1) | add(0),
    ],
    // Algorithm 4
    [
        FB_DST | NO_MOD | 1,
        modulation(1) | 1,
        FB_SRC | modulation(1) | OUTPUT,
        NO_MOD | 1,
        modulation(1) | 1,
        modulation(1) | add(0),
    ],
    // Algorithm 5
    [
        FB | 1,
        modulation(1) | OUTPUT,
        NO_MOD | 1,
        modulation(1) | add(0),
        NO_MOD | 1,
        modulation(1) | add(0),
    ],
    // Algorithm 6
    [
        FB_DST | NO_MOD | 1,
        FB_SRC | modulation(1) | OUTPUT,
        NO_MOD | 1,
        modulation(1) | add(0),
        NO_MOD | 1,
        modulation(1) | add(0),
    ],
    // Algorithm 7
    [
        FB | 1,
        modulation(1) | 1,
        NO_MOD | add(1),
        modulation(1) | OUTPUT,
        NO_MOD | 1,
        modulation(1) | add(0),
    ],
    // Algorithm 8
    [
        NO_MOD | 1,
        modulation(1) | 1,
        FB | add(1),
        modulation(1) | OUTPUT,
        NO_MOD | 1,
        modulation(1) | add(0),
    ],
    // Algorithm 9
    [
        NO_MOD | 1,
        modulation(1) | 1,
        NO_MOD | add(1),
        modulation(1) | OUTPUT,
        FB | 1,
        modulation(1) | add(0),
    ],
    // Algorithm 10
    [
        NO_MOD | 1,
        NO_MOD | add(1),
        modulation(1) | OUTPUT,
        FB | 1,
        modulation(1) | 1,
        modulation(1) | add(0),
    ],
    // Algorithm 11
    [
        FB | 1,
        NO_MOD | add(1),
        modulation(1) | OUTPUT,
        NO_MOD | 1,
        modulation(1) | 1,
        modulation(1) | add(0),
    ],
    // Algorithm 12
    [
        NO_MOD | 1,
        NO_MOD | add(1),
        NO_MOD | add(1),
        modulation(1) | OUTPUT,
        FB | 1,
        modulation(1) | add(0),
    ],
    // Algorithm 13
    [
        FB | 1,
        NO_MOD | add(1),
        NO_MOD | add(1),
        modulation(1) | OUTPUT,
        NO_MOD | 1,
        modulation(1) | add(0),
    ],
    // Algorithm 14
    [
        FB | 1,
        NO_MOD | add(1),
        modulation(1) | 1,
        modulation(1) | OUTPUT,
        NO_MOD | 1,
        modulation(1) | add(0),
    ],
    // Algorithm 15
    [
        NO_MOD | 1,
        NO_MOD | add(1),
        modulation(1) | 1,
        modulation(1) | OUTPUT,
        FB | 1,
        modulation(1) | add(0),
    ],
    // Algorithm 16
    [
        FB | 1,
        modulation(1) | 1,
        NO_MOD | 2,
        modulation(2) | add(1),
        NO_MOD | add(1),
        modulation(1) | OUTPUT,
    ],
    // Algorithm 17
    [
        NO_MOD | 1,
        modulation(1) | 1,
        NO_MOD | 2,
        modulation(2) | add(1),
        FB | add(1),
        modulation(1) | OUTPUT,
    ],
    // Algorithm 18
    [
        NO_MOD | 1,
        modulation(1) | 1,
        modulation(1) | 1,
        FB | add(1),
        NO_MOD | add(1),
        modulation(1) | OUTPUT,
    ],
    // Algorithm 19
    [
        FB | 1,
        modulation(1) | OUTPUT,
        modulation(1) | add(0),
        NO_MOD | 1,
        modulation(1) | 1,
        modulation(1) | add(0),
    ],
    // Algorithm 20
    [
        NO_MOD | 1,
        NO_MOD | add(1),
        modulation(1) | OUTPUT,
        FB | 1,
        modulation(1) | add(0),
        modulation(1) | add(0),
    ],
    // Algorithm 21
    [
        NO_MOD | 1,
        modulation(1) | OUTPUT,
        modulation(1) | add(0),
        FB | 1,
        modulation(1) | add(0),
        modulation(1) | add(0),
    ],
    // Algorithm 22
    [
        FB | 1,
        modulation(1) | OUTPUT,
        modulation(1) | add(0),
        modulation(1) | add(0),
        NO_MOD | 1,
        modulation(1) | add(0),
    ],
    // Algorithm 23
    [
        FB | 1,
        modulation(1) | OUTPUT,
        modulation(1) | add(0),
        NO_MOD | 1,
        modulation(1) | add(0),
        NO_MOD | add(0),
    ],
    // Algorithm 24
    [
        FB | 1,
        modulation(1) | OUTPUT,
        modulation(1) | add(0),
        modulation(1) | add(0),
        NO_MOD | add(0),
        NO_MOD | add(0),
    ],
    // Algorithm 25
    [
        FB | 1,
        modulation(1) | OUTPUT,
        modulation(1) | add(0),
        NO_MOD | add(0),
        NO_MOD | add(0),
        NO_MOD | add(0),
    ],
    // Algorithm 26
    [
        FB | 1,
        NO_MOD | add(1),
        modulation(1) | OUTPUT,
        NO_MOD | 1,
        modulation(1) | add(0),
        NO_MOD | add(0),
    ],
    // Algorithm 27
    [
        NO_MOD | 1,
        NO_MOD | add(1),
        modulation(1) | OUTPUT,
        FB | 1,
        modulation(1) | add(0),
        NO_MOD | add(0),
    ],
    // Algorithm 28
    [
        NO_MOD | OUTPUT,
        FB | 1,
        modulation(1) | 1,
        modulation(1) | add(0),
        NO_MOD | 1,
        modulation(1) | add(0),
    ],
    // Algorithm 29
    [
        FB | 1,
        modulation(1) | OUTPUT,
        NO_MOD | 1,
        modulation(1) | add(0),
        NO_MOD | add(0),
        NO_MOD | add(0),
    ],
    // Algorithm 30
    [
        NO_MOD | OUTPUT,
        FB | 1,
        modulation(1) | 1,
        modulation(1) | add(0),
        NO_MOD | add(0),
        NO_MOD | add(0),
    ],
    // Algorithm 31
    [
        FB | 1,
        modulation(1) | OUTPUT,
        NO_MOD | add(0),
        NO_MOD | add(0),
        NO_MOD | add(0),
        NO_MOD | add(0),
    ],
    // Algorithm 32
    [
        FB | OUTPUT,
        NO_MOD | add(0),
        NO_MOD | add(0),
        NO_MOD | add(0),
        NO_MOD | add(0),
        NO_MOD | add(0),
    ],];

/// One of the renderers Plaits instantiates: a chain of `n` operators, each
/// phase-modulating the next, with the chain's modulation source.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    /// One operator modulated by a buffer.
    External { additive: bool },
    /// One unmodulated operator.
    Unmodulated { additive: bool },
    /// One operator modulated by its own last two outputs.
    Feedback { additive: bool },
    /// Three operators whose last feeds back into the first, added to the
    /// output buffer.
    Loop3,
    /// Two operators whose last feeds back into the first, added to the
    /// output buffer.
    Loop2,
}

impl Kind {
    fn find(n: usize, modulation_source: i32, additive: bool) -> Option<Self> {
        match (n, modulation_source, additive) {
            (1, -2, additive) => Some(Self::External { additive }),
            (1, -1, additive) => Some(Self::Unmodulated { additive }),
            (1, 0, additive) => Some(Self::Feedback { additive }),
            (3, 2, true) => Some(Self::Loop3),
            (2, 1, true) => Some(Self::Loop2),
            _ => None,
        }
    }
}

/// A render call: the chain starting at operator index `first`, reading
/// buffer `input` and writing buffer `output`.
#[derive(Clone, Copy, Debug)]
pub struct Call {
    pub kind: Kind,
    pub first: usize,
    pub input: usize,
    pub output: usize,
}

/// An algorithm's render calls in order, and which operators modulate others
/// (the ones `brightness` scales).
#[derive(Clone, Copy, Debug)]
pub struct Algorithm {
    calls: [Call; 6],
    len: usize,
    pub is_modulator: [bool; 6],
}

impl Algorithm {
    pub fn calls(&self) -> &[Call] {
        &self.calls[..self.len]
    }

    /// Plaits' `Algorithms::Compile`: the longest chains it has a renderer
    /// for.
    pub fn compile(algorithm: usize) -> Self {
        let opcodes = &OPCODES[algorithm];
        let placeholder = Call {
            kind: Kind::Unmodulated { additive: false },
            first: 0,
            input: 0,
            output: 0,
        };
        let mut calls = [placeholder; 6];
        let mut len = 0;
        let mut i = 0;
        while i < 6 {
            let opcode = opcodes[i];
            let mut n = 1;
            while i + n < 6 {
                let from = opcodes[i + n - 1];
                let to = (opcodes[i + n] & SOURCE_MASK) >> 4;
                let has_additive = from & ADDITIVE_FLAG != 0;
                let broken = from & DESTINATION_MASK != to;
                if has_additive || broken {
                    // A modulation that later operators reuse (algorithms 19
                    // to 25) breaks the chain.
                    if to == opcode & DESTINATION_MASK {
                        n = 1;
                    }
                    break;
                }
                n += 1;
            }
            let call = loop {
                let out_opcode = opcodes[i + n - 1];
                let additive = out_opcode & ADDITIVE_FLAG != 0;
                let modulation_source = if opcode & SOURCE_MASK == 0 {
                    -1
                } else if opcode & SOURCE_MASK != SOURCE_FEEDBACK {
                    -2
                } else {
                    (0..n)
                        .rev()
                        .find(|&j| opcodes[i + j] & FEEDBACK_SOURCE_FLAG != 0)
                        .map_or(-3, |j| j as i32)
                };
                match Kind::find(n, modulation_source, additive) {
                    Some(kind) => {
                        break Call {
                            kind,
                            first: i,
                            input: usize::from((opcode & SOURCE_MASK) >> 4),
                            output: usize::from(out_opcode & DESTINATION_MASK),
                        };
                    }
                    None => {
                        assert!(n > 1, "algorithm {algorithm}: no renderer for operator {i}");
                        n = 1;
                    }
                }
            };
            calls[len] = call;
            len += 1;
            i += n;
        }
        Self {
            calls,
            len,
            is_modulator: opcodes.map(|opcode| opcode & DESTINATION_MASK != 0),
        }
    }
}
