/**
 * Assets bundled with the app. A patch names one by a path starting with
 * `builtin/`, which never collides with a workspace path (those start with
 * `/`). The files live in `assets/builtin`, at the path after the prefix.
 */
export const BUILTIN_PREFIX = 'builtin/';

/** A bundled wavetable, documented on the Docs panel's Built-ins page. */
export interface BuiltinWavetable {
    /** The path a patch writes, starting with `builtin/`. */
    path: string;
    /** How the frames sound and change across the table. */
    description: string;
}

/** Introduces the Mutable Instruments tables on the Built-ins page. */
export const MI_TABLES_INTRO =
    "The wavetables of Mutable Instruments' Plaits, Beads, and Braids by Emilie Gillet";

const MI_TABLES: [name: string, description: string][] = [
    ['harmonics', 'A single sine at each harmonic from 1 to 16 in turn.'],
    [
        'harmonics-wide',
        'A single sine at harmonics 2, 3, 4, 6, 8, 12, 16 and 24 in turn.',
    ],
    [
        'quadra',
        'Four consecutive odd harmonics at levels 1, 1/2, 1, 1/2. The lowest of them climbs from the 5th to the 49th harmonic across the table.',
    ],
    [
        'comb',
        'The first n harmonics at equal level, n running through 2, 3, 5, 8, 13, 21, 34 and 55.',
    ],
    [
        'pair',
        'The first n harmonics, each with a partner two octaves up at half its level, higher harmonics louder. n runs from 2 to 16 in steps of 2.',
    ],
    [
        'triangles',
        'Band-limited triangle waves stacked at neighbouring harmonics, the stack growing larger and reaching higher across the table.',
    ],
    [
        'organ',
        "Sixteen drawbar registrations of a tonewheel organ's nine drawbars, from 16' to 1'.",
    ],
    [
        'tri-saw',
        'A triangle with a saw added at harmonics 1 to 8, then a saw with a triangle added at harmonics 8 back down to 1.',
    ],
    [
        'pulse',
        'A pulse whose width narrows from 1/2 (a square) through 2/5, 1/3, 1/4, 1/5, 1/8 and 1/16 to 1/32.',
    ],
    [
        'bursts',
        'A burst of sine followed by silence. As the burst shortens, the sine inside it rises, from 2 to 32 cycles per frame.',
    ],
    [
        'band-pass',
        'A sine that fades out over the frame, like a ringing band-pass filter struck once per cycle, its pitch rising from 3 to 24 times the fundamental.',
    ],
    [
        'sine-power',
        'A sine with a 16-harmonic saw added, raised to ever higher powers so the wave pinches into sharp peaks.',
    ],
    [
        'formants',
        'Three decaying sine formants per cycle, their frequencies rising across the table.',
    ],
    [
        'digital-formants',
        'Three decaying formants, each driven hard into a soft clipper, their frequencies rising across the table.',
    ],
    ['male', "Braids' male voice waves."],
    ['choir', "Braids' choir waves."],
    ['digital', "Braids' digital waves."],
    ['drone', "Braids' drone waves."],
    ['metal', "Braids' metallic waves."],
    ['fantasy', "Braids' fantasy waves."],
    [
        'braids',
        'Sixteen consecutive Braids waves, kept in their original shape.',
    ],
    [
        'fm-octave',
        'Phase modulation with the carrier an octave above the modulator, the modulation growing from none to its deepest.',
    ],
    [
        'fm-sub',
        'Phase modulation with the modulator an octave below the carrier, the modulation growing from none to its deepest.',
    ],
    [
        'fm-bell',
        'Phase modulation with the modulator at three times the carrier, the modulation growing from none to its deepest.',
    ],
    [
        'fm-bright',
        'Phase modulation with the modulator at eight times the carrier, the modulation growing from none to its deepest.',
    ],
];

/** Every bundled wavetable WAV. */
export const BUILTIN_WAVETABLES: readonly BuiltinWavetable[] = MI_TABLES.map(
    ([name, description]) => ({
        path: `${BUILTIN_PREFIX}tables/mi/${name}.wav`,
        description,
    }),
);
