/**
 * Shared type documentation for DSL types.
 * Used by both TypeScript lib generator (JSDoc) and the Docs panel (rendered docs).
 */

import { TRACK_OPTIONS, trackOptionsDefinition } from './trackOptions';

export interface TypeMethod {
    name: string;
    signature: string;
    description: string;
    example?: string;
}

export interface TypeDocumentation {
    name: string;
    description: string;
    definition?: string;
    examples: string[];
    seeAlso: string[];
    methods?: TypeMethod[];
}

/**
 * All DSL type names that should be linkified in documentation.
 */
export const DSL_TYPE_NAMES = [
    'Signal',
    'Poly<Signal>',
    'LaneSum<Signal>',
    'Multi-output modules',
    'Pattern',
    'Collection',
    'CollectionWithRange',
    'Note',
    'HZ',
    'Chord',
    'Scale',
    'Track labels',
    'TrackOptions',
] as const;

export type DslTypeName = (typeof DSL_TYPE_NAMES)[number];

/**
 * Comprehensive documentation for all DSL types.
 */
export const TYPE_DOCS: Record<DslTypeName, TypeDocumentation> = {
    Collection: {
        definition:
            'class extends Iterable<Collection> { length: number; [index]: Collection; ... }',
        description:
            'A signal with chainable DSP methods: what every module returns, one lane per voice. ' +
            'Group signals with the $c() helper function. Supports iteration, indexing, and spreading, each giving one-lane collections. ' +
            'Methods operate on every lane.',
        examples: [
            "const osc = $sine('c4')",
            'sound: osc.amp(2.5)                   // Chain methods',
            "$lpf(osc, 'c3', { q: 4 })            // Use as input to another module",
            'mix: $c(osc1, osc2, osc3).amp(2.5)  // Halve all, send to output',
            'const voices = $c(osc1, osc2, osc3)',
            'for (const v of voices) { ... }      // Iterate over one-lane collections',
            '[...voices]                          // Spread to array',
            'voices[0]                            // Index access: a one-lane collection',
        ],
        methods: [
            {
                name: 'scope',
                signature:
                    "scope(config?: { msPerFrame?: number; triggerThreshold?: number; triggerWaitToRender?: boolean; range?: [number, number]; split?: 'vertical' | 'horizontal' }): this",
                description:
                    "Add scope visualization for all outputs in the collection, overlaid on one scope. `split: 'vertical'` stacks one scope per lane; `split: 'horizontal'` fits them side by side.",
                example: "mix: $c(osc1, osc2).scope({ split: 'vertical' })",
            },
            {
                name: 'track',
                signature: 'track(options?: TrackOptions): Track',
                description:
                    'How this signal mixes into its track: gain, pan, spread and sends. A track is heard once its statement is labelled.',
                example: 'lead: $c(osc1, osc2).track({ gain: 3, pan: -2 })',
            },
            {
                name: 'range',
                signature:
                    'range(outMin: Poly<Signal>, outMax: Poly<Signal>, inMin: Poly<Signal>, inMax: Poly<Signal>): CollectionWithRange',
                description:
                    'Remap all outputs from input range to output range. Requires explicit input min/max.',
                example: '$c(lfo1, lfo2).range(0, 1, -5, 5)',
            },
            {
                name: 'pipe',
                signature: 'pipe<T>(pipeFn: (self: this) => T): T',
                description:
                    'Pass this collection through a transform function and return the result. ' +
                    'Enables inline transforms and reusable signal-processing helpers.',
                example: 'mix: $c(osc1, osc2).pipe(all => all.amp(2.5))',
            },
            {
                name: 'pipe',
                signature:
                    'pipe<T>(pipeFn: (self: this, item: E) => T, array: E[]): Collection',
                description:
                    'Call pipeFn once for every element in the provided array, ' +
                    'collecting all results into a Collection.',
                example:
                    "mix: $c(osc1, osc2).pipe((col, cutoff) => $lpf(col, cutoff), ['200hz', '800hz', '3200hz'])",
            },
            {
                name: 'pipeMix',
                signature:
                    'pipeMix(pipeFn: (self: this) => Collection | CollectionWithRange, mix?: Poly<Signal>): Collection',
                description:
                    'Pipe this collection through a transform, then crossfade the original against the transformed signal (0 = original, 5 = transformed, default 2.5).',
                example: "mix: $c(osc1, osc2).pipeMix(s => $lpf(s, '1000hz'))",
            },
            {
                name: 'wet',
                signature: 'wet(mix?: Poly<Signal>): Collection',
                description:
                    'Every module whose first argument is a signal is also a method of this collection: `sig.lpf(...)` is `$lpf(sig, ...)`. ' +
                    '`.wet` after one crossfades the signal it was called on against the result: 0 is dry, 5 is wet, and the default 2.5 an equal mix. ' +
                    'After a stereo effect such as `.plate()`, which mixes every voice into one stereo lane, the dry voices are mixed into one too.',
                example: "keys: $saw('c3').lpf('800hz').plate().wet(1.5)",
            },
        ],
        name: 'Collection',
        seeAlso: ['CollectionWithRange', 'Poly<Signal>'],
    },

    CollectionWithRange: {
        definition:
            'class extends Iterable<CollectionWithRange> { length: number; [index]: CollectionWithRange; ... }',
        description:
            'A Collection whose range the engine knows, as LFOs, envelopes, controls, and other modulation sources return. ' +
            'Chains that keep a range, such as add(), mul(), round() and erratic(), return one too. ' +
            'The range() method maps from that range. Group ranged signals with the $r() helper function.',
        examples: [
            "const lfo = $sine('1hz').range(0, 5)  // LFO outputs 0 to +5",
            'lfo.add(1).range(-5, 5)              // Maps from 1 to 6',
            'mix: $r(lfo1, lfo2).range(0, 5)     // Each maps from its own range',
        ],
        methods: [
            {
                name: 'range',
                signature:
                    'range(outMin: Poly<Signal>, outMax: Poly<Signal>, inMin?: Poly<Signal>, inMax?: Poly<Signal>): CollectionWithRange',
                description:
                    'Remap all outputs onto a new range, from their own ranges or from inMin–inMax when given.',
                example: "lfo.range('c3', 'c5')  // Remap LFO to pitch range",
            },
        ],
        name: 'CollectionWithRange',
        seeAlso: ['Collection'],
    },

    Chord: {
        definition: '`${Note}:${string}`',
        description:
            'A chord string: a note, a colon and a chord symbol, one lane per note, lowest first. ' +
            "The symbol is the one patterns use; see $cycle's Chords section.",
        examples: [
            '"c3:M7"        // C3, E3, G3, B3',
            '"eb:m7^1"      // Eb minor 7, first inversion',
            '"g2:7b9b13sus" // an altered sus dominant',
        ],
        name: 'Chord',
        seeAlso: ['Signal', 'Note'],
    },

    Scale: {
        definition: '`${Note}(${string})`',
        description:
            'A scale string: a note with a scale in parentheses, one lane per note, lowest first. ' +
            'The parentheses hold a scale name, semitones above the root, a tuning (`just`, `pythag`), or a tuning then semitones, as in a pattern.',
        examples: [
            '"d3(dorian)"    // D dorian from D3',
            '"c(0 2 4 7 9)"  // your own scale, in semitones',
            '"a(just)"       // all 12 steps in just intonation',
        ],
        name: 'Scale',
        seeAlso: ['Signal', 'Chord', 'Note'],
    },

    HZ: {
        definition: '`${number}hz` | `${number}Hz`',
        description:
            'A frequency string specifying a value in Hertz. ' +
            'Case-insensitive suffix "hz". Converted to 1V/oct voltage internally.',
        examples: [
            '"440hz"   // A4 concert pitch',
            '"261.63Hz" // Middle C',
            '"1000hz"  // 1 kHz',
        ],
        name: 'HZ',
        seeAlso: ['Signal', 'Note'],
    },

    'Multi-output modules': {
        description:
            'A module with several outputs, such as a sequencer with a pitch, a gate and a trigger, can be used without naming one. ' +
            'Each input takes the output that suits it: a trigger input takes the trigger, a gate input the gate, and everything else the main output. ' +
            'The editor shows the output it picked beside the value; double-click that hint to write it into the code. ' +
            'Each output is also there by name behind an underscore, as in `seq._gate`; naming one yourself always uses that output, whatever the input. ' +
            'A pattern from `$` is such a module too: it is the sequencer that plays it.',
        examples: [
            'const seq = $`c e g a`\nmelody: $saw(seq).amp($perc(seq))   // pitch for the saw, triggers for the envelope',
            'const seq = $`c3 ~ g3 ~`\nmelody: $sine(seq).amp(seq.adsr())   // the gate opens the envelope',
            'const seq = $`c e g a`\nmelody: $sine(seq).amp($adsr(seq._gate)).amp(seq._velocity)   // named outputs are used as they are',
        ],
        name: 'Multi-output modules',
        seeAlso: ['Pattern', 'Collection', 'Signal'],
    },

    Pattern: {
        description:
            'A mini-notation pattern, made by `$` or `$arrange`. A pattern is the sequencer that plays it: use it wherever a module goes, read its outputs (`_cv`, `_gate`, `_trig`, `_velocity`) and chain any Collection method or module. ' +
            'Every such use of one pattern shares one sequencer. ' +
            'The pattern methods each give a new pattern; on a scale pattern (one made by `.scale`) they give a scale pattern, which `.trans` transposes. To play a pattern with other sequencer settings, pass it to `$cycle` or call `.cycle(...)`, which makes a sequencer of its own.',
        examples: [
            'const seq = $`c e g a`\nmelody: $saw(seq).amp($perc(seq))   // one sequencer: pitch for the saw, triggers for the envelope',
            "melody: $`c3 ~ g3 ~`.fast(2).saw().lpf('800hz')   // pattern methods, then chain modules",
            "melody: $`0 2 4 7`.scale('c(major)').trans('<0 3>').saw()   // scale degrees, transposed every other cycle",
        ],
        methods: [
            {
                description: 'Speed the pattern up by a factor',
                example: '$`c e g`.fast(2)',
                name: 'fast',
                signature: 'fast(factor: number | string): Pattern',
            },
            {
                description: 'Slow the pattern down by a factor',
                example: '$`c e g`.slow(2)',
                name: 'slow',
                signature: 'slow(factor: number | string): Pattern',
            },
            {
                description:
                    'Take the rhythm from a boolean pattern, sampling this pattern at each onset',
                example: "$`c e`.struct('x ~ x x')",
                name: 'struct',
                signature: 'struct(boolPattern: string): Pattern',
            },
            {
                description:
                    'Play this pattern at chosen beats of a divided cycle',
                example: "$`c2`.beat('0,7,10', 16)",
                name: 'beat',
                signature:
                    'beat(t: number | string, div: number | string): Pattern',
            },
            {
                description: 'Glide into each note over a length in cycles',
                example: '$`c e g`.glide(0.1)',
                name: 'glide',
                signature:
                    'glide(g: number | string, curve?: GlideCurve, mode?: GlideMode): Pattern',
            },
            {
                description:
                    "Read this pattern's atoms as degrees of a scale, or change a scale pattern's scale",
                example: "$`0 2 4`.scale('c(major)')",
                name: 'scale',
                signature: 'scale(scale: string): ScalePattern',
            },
            {
                description:
                    'Transpose a scale pattern by another scale-degree pattern',
                example: "$`0 2 4`.scale('c(major)').trans('0 7')",
                name: 'trans',
                signature: 'trans(rhs: string): ScalePattern',
            },
            {
                description:
                    'Play the pattern from a playhead position in cycles, in place of the global clock',
                example: '$`c e g b`.playhead($clock._playhead.mul(0.5))',
                name: 'playhead',
                signature: 'playhead(playhead: LaneSum<Signal>): Pattern',
            },
            {
                description:
                    'Loop the stretch of the pattern `length` cycles long from cycle `offset`',
                example: '$`<c e g b>`.ribbon(1, 2)',
                name: 'ribbon',
                signature: 'ribbon(offset: number, length: number): Pattern',
            },
            {
                description:
                    'Play this pattern through a `$cycle` of its own, with `$cycle` settings',
                example: '$`c e g`.cycle({ ribbon: [0, 2] })',
                name: 'cycle',
                signature: 'cycle(config?): CycleOutputs',
            },
        ],
        name: 'Pattern',
        seeAlso: ['Multi-output modules', 'Collection'],
    },

    'LaneSum<Signal>': {
        definition: 'Signal | Signal[]',
        description:
            'A signal input that accepts polyphonic connections but sums all lanes down to a single mono value. ' +
            'Takes what Poly<Signal> takes except a signal group: the module will not produce per-voice output from this parameter. ' +
            'Used for control parameters like tempo, stereo width, or math variables where a single combined value is needed.',
        examples: [
            '$clockDivider(clock._trigger, 4)                // Clock signal summed to mono',
            '$pan(osc, 0, { spread: lfo })                    // Spread control summed to mono',
            '$math("x + y", { x: osc1, y: osc2 })            // Variables summed to single values',
        ],
        name: 'LaneSum<Signal>',
        seeAlso: ['Signal', 'Poly<Signal>', 'Collection'],
    },

    Note: {
        definition: '`${NoteName}${Accidental}${Octave}`',
        description:
            'A musical note string in scientific pitch notation. ' +
            'Consists of a note name (A-G or a-g), optional accidental (#/b), and optional octave number. ' +
            'If octave is omitted, defaults to octave 4.',
        examples: [
            '"C4"   // Middle C',
            '"A#3"  // A sharp in octave 3',
            '"Bb5"  // B flat in octave 5',
            '"G"    // G4 (octave 4 is default)',
            '"c#"    // C#4 (octave 4 is default)',
        ],
        name: 'Note',
        seeAlso: ['Signal', 'HZ'],
    },

    'Poly<Signal>': {
        definition: 'Signal | Signal[] | SignalGroup',
        description:
            'A potentially multi-lane signal. Can be an array of Signals for polyphonic patches; ' +
            'every lane of every Collection in it becomes a voice. When used as input to a module, arrays are expanded to create multiple voices.',
        examples: [
            '$saw(["C3", "E3", "G3"])                // 3-voice chord',
            '$saw([...$sine("1hz"), ...$sine("2hz")]) // Spread lanes into voices',
            '$mix([osc1, osc2, osc3])                // Several signals',
        ],
        name: 'Poly<Signal>',
        seeAlso: ['Signal', 'LaneSum<Signal>', 'Collection'],
    },

    Signal: {
        definition:
            'number | Note | HZ | Chord | Scale | Collection | CollectionWithRange',
        description:
            'An audio signal value. This is the fundamental type for all audio connections in the modular system. ' +
            'Signals follow the 1V/octave convention where 0V corresponds to C4 (~261.63 Hz).',
        examples: [
            '$sine("C4")           // Note string - converted to 1V/oct',
            '$sine(440)            // Number - constant voltage',
            '$sine("440hz")        // Hz string - converted to voltage',
            '$saw("c3:M7")         // Chord string - one lane per note',
            '$saw("d3(dorian)")    // Scale string - one lane per note',
            '$sine(lfo)            // Collection from another module',
        ],
        name: 'Signal',
        seeAlso: [
            'Poly<Signal>',
            'LaneSum<Signal>',
            'Collection',
            'Note',
            'HZ',
            'Chord',
            'Scale',
        ],
    },

    'Track labels': {
        definition: '[_ | S_ | S__]name[$channel | $channelm | $group]: signal',
        description:
            'A label in front of a top-level statement sends its signal to the mixer as a track. ' +
            "The name is the track's name in the mixer, with underscores shown as spaces, and reads as the track's final signal in the rest of the patch " +
            "(and, for a track that starts from `$bus()`, as the bus other tracks send to). `_name` mutes the track, `S_name` (or `s_name`) solos it, and `S__name` (or `_S_name`) does both, in which case it plays, as a soloed track does; the mixer's mute and solo buttons rewrite this prefix. " +
            '`name$2` plays the track in stereo on channels 2 and 3, and `name$2m` in mono on channel 2; without a suffix a track plays in stereo on channels 0 and 1. ' +
            '`name$drums` plays the track into the group track `drums` instead of the output: a group is a track that starts from `$group()`, and it alone reaches the output, through its own suffix. ' +
            'Names are unique, and any other label is an error. An unlabelled statement is not heard.',
        examples: [
            "lead: $saw('c3')                 // Stereo on channels 0 and 1",
            "_pad$2: $sine('c4:M7')  // Muted, on channels 2 and 3",
            'S_click$4m: $noise()             // Soloed, mono on channel 4',
            'kick$drums: $tr808.kick($`c ~ c ~`)\nhats$drums: $tr808.hiHat($`c*8`)\ndrums: $group().comp()  // Both drums through one compressor',
            "kick: $tr808.kick($`c ~ c ~`)\nbass: $saw('c2').amp($motionFollower(kick).range(5, 0))  // Ducks under the kick",
        ],
        name: 'Track labels',
        seeAlso: ['TrackOptions', 'Collection'],
    },

    TrackOptions: {
        definition: trackOptionsDefinition(TRACK_OPTIONS),
        description:
            'Options of `.track()`: how a track mixes into the mixer. Its name, mute, solo and channels come from its label. ' +
            "The mixer's gain and pan controls rewrite these options.",
        examples: [
            "tone: $sine('c').track({ gain: 2.5 })           // 50% gain",
            "tone: $sine('c').track({ pan: -2.5 })           // Pan left",
            "chord: $saw('c3:m7').track({ spread: 5 })  // Lanes fanned edge to edge",
            "tone: $sine('c').track({ gain: $perc($pulse('8hz')), pan: $sine('1hz') })  // Modulated",
            "tone: $sine('c').track({ sends: [verb, [echo, 2]] })  // Also feed two buses",
        ],
        name: 'TrackOptions',
        seeAlso: ['Track labels', 'Collection', 'Poly<Signal>'],
    },
};

/**
 * Check if a string is a known DSL type name.
 */
export function isDslType(name: string): name is DslTypeName {
    return DSL_TYPE_NAMES.includes(name as DslTypeName);
}

/**
 * Get documentation for a DSL type by name.
 */
export function getTypeDoc(name: string): TypeDocumentation | undefined {
    if (isDslType(name)) {
        return TYPE_DOCS[name];
    }
    return undefined;
}
