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
        definition: 'number | Note | HZ | Collection | CollectionWithRange',
        description:
            'An audio signal value. This is the fundamental type for all audio connections in the modular system. ' +
            'Signals follow the 1V/octave convention where 0V corresponds to C4 (~261.63 Hz).',
        examples: [
            '$sine("C4")           // Note string - converted to 1V/oct',
            '$sine(440)            // Number - constant voltage',
            '$sine("440hz")        // Hz string - converted to voltage',
            '$sine(lfo)            // Collection from another module',
        ],
        name: 'Signal',
        seeAlso: [
            'Poly<Signal>',
            'LaneSum<Signal>',
            'Collection',
            'Note',
            'HZ',
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
            "_pad$2: $sine($chord('C4[maj7]'))  // Muted, on channels 2 and 3",
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
            "chord: $saw($chord('C3[m7]')).track({ spread: 5 })  // Lanes fanned edge to edge",
            "tone: $sine('c').track({ gain: $perc($pulse('8hz')), pan: $sine('1hz') })  // Modulated",
            "tone: $sine('c').track({ sends: [verb, [echo, 2]] })  // Also feed two buses",
        ],
        name: 'TrackOptions',
        seeAlso: ['Track labels', 'Collection', 'Poly<Signal>'],
    },
};

export interface GlobalFunctionDoc {
    name: string;
    signature: string;
    description: string;
    params?: { name: string; type: string; description: string }[];
    returns?: string;
    examples: string[];
    group: string;
}

/**
 * Documentation for all DSL global functions and helpers.
 */
export const GLOBAL_DOCS: GlobalFunctionDoc[] = [
    // ---- Helpers ----
    {
        description:
            'Convert a frequency in Hertz to a 1V/octave voltage value. Numbers and number arrays convert to voltage values; a frequency that includes a signal converts sample by sample, with frequencies at or below 0 Hz read as 0.0001 Hz. Note and Hz strings are already pitches and are rejected.',
        examples: [
            '$hz(440)    // A4',
            '$hz(261.63) // ~C4',
            '$hz([220, 330]) // two voices',
            "$hz($slider('pitch', 220, 100, 500)) // a slider in Hz",
        ],
        group: 'Helpers',
        name: '$hz',
        params: [
            {
                name: 'frequency',
                type: 'number | Poly<number>',
                description: 'Frequency in Hz',
            },
        ],
        returns:
            'A voltage value for a number, one per element for a number array, otherwise a Collection',
        signature:
            '$hz(frequency: number | Poly<number>): number | number[] | Collection',
    },
    {
        description: 'Convert a note name string to a 1V/octave voltage value.',
        examples: ['$note("C4")  // Middle C', '$note("A4")  // 440 Hz'],
        group: 'Helpers',
        name: '$note',
        params: [
            {
                name: 'noteName',
                type: 'string',
                description: 'Note name like "C4", "A#3", "Bb5"',
            },
        ],
        returns: 'Voltage value usable as a Signal',
        signature: '$note(noteName: string): number',
    },
    {
        description:
            'Create a Collection holding the lanes of every argument, in order. Collections support chainable DSP methods, indexing, and spreading.',
        examples: [
            'mix: $c(osc1, osc2).amp(0.5)',
            '$c(osc1, osc2)[0]  // index access: a one-lane collection',
        ],
        group: 'Helpers',
        name: '$c',
        signature: '$c(...args: (Signal | Signal[])[]): Collection',
    },
    {
        description:
            'Create a CollectionWithRange holding the lanes of every ranged argument. The range() method maps each from its own range.',
        examples: ['mix: $r(lfo1, lfo2).range(0, 5)'],
        group: 'Helpers',
        name: '$r',
        signature:
            '$r(...args: (CollectionWithRange | CollectionWithRange[])[]): CollectionWithRange',
    },
    {
        description:
            'Compute the Cartesian product of the given arrays. Returns every possible combination of one element from each array.',
        examples: [
            "$cartesian([220, 440], ['sine', 'saw'])\n// → [[220,'sine'],[220,'saw'],[440,'sine'],[440,'saw']]",
            "tones: $cartesian([1, 2], ['a', 'b']).pipe(\n  (osc, [freq, shape]) => $oscillator({ freq, shape }),\n)",
        ],
        group: 'Helpers',
        name: '$cartesian',
        signature:
            '$cartesian<A extends unknown[][]>(...arrays: A): ElementsOf<A>[]',
    },
    {
        description:
            'Tag a parameter value into signal group 1 (also $g2, $g3). Parameters in different groups multiply — the module gets the cartesian product of the group widths as voices — while parameters in the same group cycle together. Untagged parameters are group 0. Lower-numbered groups vary fastest. The product may not exceed 64 lanes, and $gN must wrap the entire parameter value.',
        examples: [
            'tone: $sine($g1(["c3", "e3", "g3"]), { phaseOffset: $g2([0, 0.25]) })\n// 3 x 2 = 6 voices',
        ],
        group: 'Helpers',
        name: '$g1',
        signature: '$g1(signals: Poly<Signal>): SignalGroup',
    },
    {
        description: 'Tag a parameter value into signal group 2 — see $g1.',
        examples: [
            'tone: $sine($g1(["c3", "e3", "g3"]), { phaseOffset: $g2([0, 0.25]) })',
        ],
        group: 'Helpers',
        name: '$g2',
        signature: '$g2(signals: Poly<Signal>): SignalGroup',
    },
    {
        description: 'Tag a parameter value into signal group 3 — see $g1.',
        examples: [
            'tone: $saw($g1(["c2", "c3"]), { shape: $g2([0, 2.5]), phaseOffset: $g3([0, 0.5]) })',
        ],
        group: 'Helpers',
        name: '$g3',
        signature: '$g3(signals: Poly<Signal>): SignalGroup',
    },
    // ---- Global Settings ----
    {
        description: 'Set the global tempo for the root clock.',
        examples: ['$setTempo(120)  // 120 BPM', '$setTempo(140)  // 140 BPM'],
        group: 'Global Settings',
        name: '$setTempo',
        params: [
            { name: 'tempo', type: 'number', description: 'Tempo in BPM' },
        ],
        signature: '$setTempo(tempo: number): void',
    },
    {
        description:
            'Set the time signature for the root clock. Both values must be positive integers.',
        examples: [
            '$setTimeSignature(4, 4)  // 4/4 (default)',
            '$setTimeSignature(3, 4)  // 3/4 waltz',
            '$setTimeSignature(7, 8)  // 7/8 asymmetric',
        ],
        group: 'Global Settings',
        name: '$setTimeSignature',
        params: [
            {
                name: 'numerator',
                type: 'number',
                description: 'Beats per bar (e.g. 3, 4, 6, 7)',
            },
            {
                name: 'denominator',
                type: 'number',
                description:
                    'Beat value (e.g. 4 for quarter note, 8 for eighth note)',
            },
        ],
        signature:
            '$setTimeSignature(numerator: number, denominator: number): void',
    },
    {
        description:
            'Set the global groove. Every $cycle and $drumMap starts its notes on it, keeping their lengths, unless given its own groove or playhead.',
        examples: [
            '$setGroove($swing(0.6))  // 60% sixteenth swing',
            '$setGroove({ timing: [0, 0.2, 0, 0.15], velocity: [1, 0.6, 0.8, 0.5] })',
        ],
        group: 'Global Settings',
        name: '$setGroove',
        params: [
            {
                name: 'template',
                type: 'GrooveTemplate',
                description:
                    'Per-step timing offsets (grid steps) and velocities (0-1), plus grid, amount, velocityAmount, random and seed',
            },
        ],
        signature: '$setGroove(template: GrooveTemplate): void',
    },
    {
        description:
            'Swing groove template: each pair of grid steps is split ratio : (1 - ratio).',
        examples: [
            '$setGroove($swing(0.58))  // light sixteenth swing',
            '$setGroove($swing(0.66, { grid: 8 }))  // triplet-feel eighths',
            '$setGroove($swing(0.6, { humanize: 0.05 }))  // swing with a little looseness',
        ],
        group: 'Global Settings',
        name: '$swing',
        params: [
            {
                name: 'ratio',
                type: 'number',
                description:
                    'Share of each pair given to its first step (0.5 = straight)',
            },
            {
                name: 'config',
                type: '{ grid?: number; humanize?: number }',
                description:
                    'grid: grid steps per cycle (default 16); humanize: largest random timing offset per step, in grid steps (default 0)',
            },
        ],
        signature:
            '$swing(ratio: number, config?: { grid?: number; humanize?: number }): GrooveTemplate',
    },
    {
        description:
            'Set the main gain applied to the final mix: 5, unity, is the default, and each step of 1 is 12 dB.',
        examples: [
            '$setMainGain(3.5) // -18 dB',
            '$setMainGain(5.0) // unity gain (default)',
            "$setMainGain($sine('1hz')) // modulated gain",
        ],
        group: 'Global Settings',
        name: '$setMainGain',
        params: [
            {
                name: 'gain',
                type: 'LaneSum<Signal>',
                description: 'Gain level (2.5 default, 5.0 unity)',
            },
        ],
        signature: '$setMainGain(gain: LaneSum<Signal>): void',
    },
    // ---- Controls -----
    {
        description:
            'Create a UI slider bound to a signal module. The slider appears in the Control panel as soon as it is written. Dragging it rewrites the source value, and drives the audio engine in real time once the patch is evaluated.',
        examples: [
            'const vol = $slider("Volume", 0.5, 0, 1);\ntone: $sine(440).amp(vol);',
            'const cutoff = $slider("Cutoff", 1000, 100, 8000);\ntone: $saw(440).pipe(s => $lpf(s, cutoff));',
        ],
        group: 'Controls',
        name: '$slider',
        params: [
            {
                name: 'label',
                type: 'string',
                description:
                    'Display label (must be a string literal, unique within its group)',
            },
            {
                name: 'value',
                type: 'number | string',
                description:
                    'Initial value: a number, an hz string like "440hz", or a note like "c4" (must be a literal)',
            },
            {
                name: 'min',
                type: 'number | string',
                description:
                    'Minimum value, in the same unit (must be a literal)',
            },
            {
                name: 'max',
                type: 'number | string',
                description:
                    'Maximum value, in the same unit (must be a literal)',
            },
            {
                name: 'group',
                type: 'ControlGroup',
                description:
                    'Group to place the control in: a $cGroup(...) call or a const bound to one (optional)',
            },
        ],
        returns:
            'CollectionWithRange carrying the current slider value (range [min, max])',
        signature:
            '$slider(label: string, value: number | string, min: number | string, max: number | string, group?: ControlGroup): CollectionWithRange',
    },
    {
        description:
            'Create a momentary gate button in the Control panel: 5V while held, 0V otherwise. Chain through .hold for a fixed-length trigger per press.',
        examples: [
            'lead: $sine("c4").amp($adsr($btn("play")));',
            '// fixed 100 ms trigger per press, however long the button is held\nlead: $sine("c4").amp($adsr($btn("hit").hold(0.1)));',
        ],
        group: 'Controls',
        name: '$btn',
        params: [
            {
                name: 'label',
                type: 'string',
                description:
                    'Display label (must be a string literal, unique within its group)',
            },
            {
                name: 'group',
                type: 'ControlGroup',
                description:
                    'Group to place the control in: a $cGroup(...) call or a const bound to one (optional)',
            },
        ],
        returns:
            'CollectionWithRange carrying the button output (range [0, 5])',
        signature:
            '$btn(label: string, group?: ControlGroup): CollectionWithRange',
    },
    {
        description:
            'Create a latched toggle button in the Control panel: clicking flips between 0V and 5V and rewrites the initial-state literal in the source, so the state survives re-execution.',
        examples: ['bass: $saw("c2").amp($toggleBtn("drone", false));'],
        group: 'Controls',
        name: '$toggleBtn',
        params: [
            {
                name: 'label',
                type: 'string',
                description:
                    'Display label (must be a string literal, unique within its group)',
            },
            {
                name: 'initial',
                type: 'boolean',
                description: 'Initial state (must be a true/false literal)',
            },
            {
                name: 'group',
                type: 'ControlGroup',
                description:
                    'Group to place the control in: a $cGroup(...) call or a const bound to one (optional)',
            },
        ],
        returns:
            'CollectionWithRange carrying the button output (range [0, 5])',
        signature:
            '$toggleBtn(label: string, initial: boolean, group?: ControlGroup): CollectionWithRange',
    },
    {
        description:
            'Create a group of controls. Controls given the returned group as their last argument appear together in the Control panel under a collapsible header; groups nest through params.group. Clicking the header rewrites the collapsed literal in the source. Control labels need only be unique within their group.',
        examples: [
            'const voice = $cGroup("Voice");\ntone: $sine($slider("Pitch", "c4", "c2", "c6", voice)).amp($adsr($btn("Play", voice)));',
            'const synth = $cGroup("Synth");\nconst amp = $cGroup("Amp", { collapsed: true, group: synth });\ntone: $saw($slider("Root", "c3", "c2", "c5", synth)).amp($slider("Level", 0.5, 0, 1, amp));',
        ],
        group: 'Controls',
        name: '$cGroup',
        params: [
            {
                name: 'label',
                type: 'string',
                description:
                    'Display label (must be a string literal, unique among its siblings)',
            },
            {
                name: 'params',
                type: '{ collapsed?: boolean, group?: ControlGroup }',
                description:
                    'collapsed: whether the group starts collapsed (a true/false literal); group: a parent group to nest in. Must be an object literal; optional',
            },
        ],
        returns:
            "A ControlGroup reference. Pass the $cGroup(...) call or a const bound to it as a control's group argument.",
        signature:
            '$cGroup(label: string, params?: { collapsed?: boolean, group?: ControlGroup }): ControlGroup',
    },
    // ---- Advanced ----
    {
        description:
            'Create placeholder signals that can be assigned later. Useful for feedback loops.',
        examples: [
            "const fb = $deferred();\nconst echoed = $delay($noise('white').amp(fb[0]), 0.25);\nfb.set(echoed);\nnoise: echoed;",
        ],
        group: 'Advanced',
        name: '$deferred',
        params: [
            {
                name: 'lanes',
                type: 'number',
                description: 'Number of deferred outputs (1-64, default 1)',
            },
        ],
        returns: 'DeferredCollection',
        signature: '$deferred(lanes?: number): DeferredCollection',
    },

    {
        description:
            "A group: one stereo lane, the sum of every track whose label ends in `$` and the name of this group's track. Those tracks play into the group after their own pan, gain, mute and solo, and reach the output only through it. " +
            'Chain effects on it and label the result: that track is the group. It plays to the main output, to the channels of its own `$N` / `$Nm` suffix, or into another group, so groups nest. ' +
            'A group has one track, a group track can send to buses, and a bus track cannot play into a group. The mixer shows a group with its tracks beside it, and can fold them away.',
        examples: [
            'kick$drums: $tr808.kick($`c ~ c ~`)\nhats$drums: $tr808.hiHat($`c*8`)\ndrums: $group().comp().track({ gain: 4 })',
            "// Groups nest: drums and bass share a rhythm group on channels 2 and 3\nkick$drums: $tr808.kick($`c ~ c ~`)\ndrums$rhythm: $group()\nbass$rhythm: $saw('c2')\nrhythm$2: $group().lpf('c7')",
        ],
        group: 'Advanced',
        name: '$group',
        params: [],
        returns: "The group's signal, to chain on and label",
        signature: '$group(): GroupSignal',
    },
    {
        description:
            'A send-return bus: one stereo lane, the mix of every signal sent to it through the `sends` option of `.track()`. ' +
            "Chain effects on it and label the result to hear it. That track is the bus: its name is what `sends` takes, and reads as the track's signal like any other name. " +
            'A bus has one track, and a bus track cannot send to another bus or be soloed: it keeps playing while other tracks are soloed, so a soloed track is heard with its sends. The mixer shows bus tracks on the right, just left of the main strip.',
        examples: [
            "fx: $bus().plate()\ntone: $saw('a').track({ sends: [fx, 3] })\nbass: $sine('a2').track({ sends: fx })",
            "// One track feeding two buses at different levels\nverb: $bus().plate().track({ gain: 3 })\necho: $bus().delay(0.25)\nkeys: $saw('c3').track({ sends: [[verb, 2], [echo, 4]] })",
        ],
        group: 'Advanced',
        name: '$bus',
        params: [],
        returns: "The bus's signal, to chain on and label",
        signature: '$bus(): BusSignal',
    },
    {
        description:
            'Tap the track named `name`: one lane of its audio after pan, gain and mute/solo, stereo for a stereo track, ' +
            'with `.active` (0–5V gate), `.level` (gain, 0–10), and `.panning` (-5–5) as ranged signals. ' +
            'Follows live mixer changes. A track label already binds its name to this tap; `$trackTap` is for a name held in a string. A name no track has is an error.',
        examples: [
            "bass: $saw('c2')\nconst tap = $trackTap('bass')\nlead: $sine('c5').gain(tap.level).track({ pan: tap.panning })\nbell: $sine('g5').gain(tap.active)",
        ],
        group: 'Advanced',
        name: '$trackTap',
        params: [
            {
                name: 'name',
                type: 'string',
                description: 'Name of the track to follow',
            },
        ],
        returns: 'TrackTap',
        signature: '$trackTap(name: string): TrackTap',
    },
    {
        description:
            'Set a custom processor applied to the final mix just before the main gain. ' +
            'The callback receives the fully mixed Collection and should return a processed signal.',
        examples: [
            `$setEndOfChainCb((mix) => $lpf(mix, '2000hz'));`,
            '$setEndOfChainCb((mix) => mix.scope());',
        ],
        group: 'Advanced',
        name: '$setEndOfChainCb',
        params: [
            {
                name: 'cb',
                type: '(mixed: Collection) => Collection | CollectionWithRange',
                description: 'Transform applied to the final mix',
            },
        ],
        signature:
            '$setEndOfChainCb(cb: (mixed: Collection) => Collection | CollectionWithRange): void',
    },
];

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
