/**
 * The DSL's global functions: their TypeScript declarations and docs, which
 * the generated Monaco typings and the Docs panel's Globals page both read.
 * `$scopeXY` is documented with the video namespace (`videoDocs.ts`).
 */
import type { FunctionDoc, ReferenceEntry } from './referenceDocs';
import { functionToReferenceEntry } from './referenceDocs';

const CONTROL_LABEL = 'Display label, a string literal unique within its group';
const CONTROL_GROUP =
    'The group to place the control in: a `$cGroup(...)` call or a const bound to one';

export const GLOBALS: FunctionDoc[] = [
    {
        declarations: [
            '$hzToV(frequency: number): number',
            '$hzToV(frequency: number[]): number[]',
            '$hzToV(frequency: Poly<number>): Collection',
            '$hzToV(frequency: number | Poly<number>): number | number[] | Collection',
        ],
        docs: {
            categories: ['pitch'],
            description:
                'Converts a frequency in Hz to a V/Oct pitch. A number converts as the patch builds, and a frequency that includes a signal converts sample by sample, reading 0 Hz and below as 0.0001 Hz. Note and Hz strings are already pitches, so `$hzToV` rejects them.',
            examples: [
                {
                    code: 'tone: $sine($hzToV(440))',
                    title: 'A4 from its frequency',
                },
                {
                    code: 'chord: $saw($hzToV([220, 330, 440]))',
                    title: 'Three voices in Hz',
                },
                {
                    code: "tone: $sine($hzToV($slider('Pitch', 220, 100, 500)))",
                    title: 'A slider in Hz',
                },
            ],
            sections: [],
            summary: 'Hz to V/Oct pitch',
        },
        params: { frequency: 'Frequency in Hz' },
        returns: { description: 'The pitch, usable as a signal' },
    },
    {
        declarations: ['$note(noteName: string): number'],
        docs: {
            categories: ['pitch'],
            description: 'Converts a note name to a V/Oct pitch.',
            examples: [
                { code: "tone: $sine($note('c4'))", title: 'Middle C' },
                {
                    code: "tone: $sine($note('c4') + 7 / 12)",
                    title: 'A fifth above middle C',
                },
            ],
            sections: [],
            summary: 'Note name to V/Oct pitch',
        },
        params: { noteName: 'A note such as `"C4"`, `"A#3"` or `"Bb5"`' },
        returns: { description: 'The pitch, usable as a signal' },
    },
    {
        declarations: ['$c(...args: (Signal | Signal[])[]): Collection'],
        docs: {
            categories: ['utility'],
            description:
                'Gathers the lanes of every argument, in order, into one collection, which takes chain methods, indexing and spreading. Bare numbers and note, Hz, chord or scale strings become `$signal`s, so they mix in beside module outputs.',
            examples: [
                {
                    code: "keys: $c($sine('c3'), $sine('e3')).amp(0.5)",
                    title: 'Two sines, one level',
                },
                {
                    code: "lead: $saw($c(440, 'c4', $sine('e3')))",
                    title: 'Numbers and notes beside signals',
                },
                {
                    code: "melody: $saw($c($`c4 e4`, 'g4'))",
                    title: 'A pattern contributes its voices',
                },
                {
                    code: "tone: $c($sine('c3'), $sine('e3'), $sine('g3'))[0]",
                    title: 'Index one lane',
                },
            ],
            sections: [],
            summary: 'Group signals into lanes',
        },
        params: { args: 'Signals, or arrays of them' },
        returns: { description: 'Every argument’s lanes' },
    },
    {
        declarations: [
            '$r(...args: (CollectionWithRange | CollectionWithRange[])[]): CollectionWithRange',
        ],
        docs: {
            categories: ['utility'],
            description:
                'Gathers ranged signals into one collection like `$c`, each lane keeping its own range, so `.range()` maps each from where it swings.',
            examples: [
                {
                    code: "tone: $sine(\n  $r($sine('1hz'), $sine('2hz')).range('c3', 'c5'),\n)",
                    title: 'Two LFOs sweeping pitch',
                },
            ],
            sections: [],
            summary: 'Group ranged signals',
        },
        params: { args: 'Ranged signals, or arrays of them' },
        returns: { description: 'Every argument’s lanes, with their ranges' },
    },
    {
        declarations: [
            '$cartesian<A extends unknown[][]>(...arrays: A): ElementsOf<A>[]',
        ],
        docs: {
            categories: ['utility'],
            description:
                'Every combination of one element from each array, as typed tuples, for fanning voices across several dimensions at once.',
            examples: [
                {
                    code: "voices: $mix(\n  $cartesian([220, 440], ['sine', 'saw']).map(\n    ([freq, shape]) =>\n      shape === 'sine'\n        ? $sine($hzToV(freq))\n        : $saw($hzToV(freq)),\n  ),\n)",
                    title: 'Every pitch on every waveform',
                },
            ],
            sections: [],
            summary: 'Every combination of arrays',
        },
        params: { arrays: 'Arrays to combine' },
        returns: { description: 'One tuple per combination' },
    },
    ...[1, 2, 3].map((n): FunctionDoc => ({
        declarations: [`$g${n}(signals: Poly<Signal>): SignalGroup`],
        docs: {
            categories: ['utility'],
            description:
                n === 1
                    ? 'Tags a param value into signal group 1 (and `$g2`, `$g3` into groups 2 and 3). Params in different groups multiply, the module playing every combination of their lanes, while params in one group cycle together and untagged ones are group 0. Lower groups vary fastest, at most 64 lanes in all, and `$gN` must wrap the whole param value.'
                    : `Tags a param value into signal group ${n}; see \`$g1\`.`,
            examples: [
                n === 3
                    ? {
                          code: "tone: $saw($g1(['c2', 'c3']), {\n  shape: $g2([0, 2.5]),\n  phaseOffset: $g3([0, 0.5]),\n})",
                          title: '2 × 2 × 2 = 8 voices',
                      }
                    : {
                          code: "tone: $sine($g1(['c3', 'e3', 'g3']), {\n  phaseOffset: $g2([0, 0.25]),\n})",
                          title: '3 pitches × 2 phases = 6 voices',
                      },
            ],
            sections: [],
            summary: `Signal group ${n}`,
        },
        params: { signals: 'The whole param value' },
        returns: { description: 'The value, tagged with its group' },
    })),
    {
        declarations: [
            '$cross(count: number, playhead: LaneSum<Signal>, range?: [number, number], interpolationType?: Parameters<typeof $keyframes>[1]["interpolationType"]): Collection',
        ],
        docs: {
            categories: ['mixer'],
            description:
                'Crossfade weights for `count` voices: as `playhead` moves from 0 to 1, each lane in turn rises to the on weight while the others fall to off.',
            examples: [
                {
                    code: "const osc = $sine(['c', 'e', 'g'])\nconst weights = $cross(\n  osc.length,\n  $sine('0.25hz').range(0, 1),\n)\nchord: osc.amp(weights)",
                    title: 'A slow fade through a chord',
                },
            ],
            sections: [],
            summary: 'Crossfade weights',
        },
        params: {
            count: 'How many weights, one lane each',
            interpolationType: {
                description: 'Easing between neighbouring weights',
                meta: 'default linear',
            },
            playhead: {
                description:
                    'Position across the lanes, e.g. an LFO through `.range(0, 1)`; it wraps',
                meta: '0–1',
            },
            range: {
                description: 'The `[off, on]` weights',
                meta: 'default [0, 5]',
            },
        },
        returns: { description: 'One weight per lane' },
    },
    {
        declarations: ['$deferred(lanes?: number): DeferredCollection'],
        docs: {
            categories: ['utility'],
            description:
                'Placeholder signals assigned later with `.set()`, for feedback loops and forward references. A cycle needs a delay in it to play.',
            examples: [
                {
                    code: "const fb = $deferred()\nconst echoed = $delay($noise('white').amp(fb[0]), 0.25)\nfb.set(echoed)\nnoise: echoed",
                    title: 'Noise feeding back through a delay',
                },
            ],
            sections: [],
            summary: 'Placeholder for feedback',
        },
        params: {
            lanes: {
                description: 'How many placeholders',
                meta: '1–64, default 1',
            },
        },
        returns: {
            description: 'The placeholders, to read now and `.set()` later',
        },
    },
    {
        declarations: ['$wavMeta(path: string): WavMeta'],
        docs: {
            categories: ['sampler'],
            description:
                'Reads a WAV file’s format, tempo, loops and cue points, so a patch can follow the file it plays.',
            examples: [
                {
                    code: "const loop = '/wavs/loop.wav'\n$setTempo($wavMeta(loop).bpm ?? 120)\nsample: $sampler(loop, $clock._barTrigger)",
                    title: 'A loop at its recorded tempo',
                },
            ],
            sections: [],
            summary: 'WAV file details',
        },
        params: {
            path: "The file, from the workspace folder, e.g. `'/wavs/loop.wav'`",
        },
        returns: { description: 'The file’s details' },
    },
    {
        declarations: [
            '$(source: string): Pattern',
            '$(strings: TemplateStringsArray, ...values: unknown[]): Pattern',
        ],
        docs: {
            categories: ['sequencer'],
            description:
                'Parses mini-notation into a pattern, called with a string or as a template tag: `` $`c4 e4 g4` `` is `$("c4 e4 g4")`. A pattern plays through `$cycle` and chains like a signal; its syntax is in the `$cycle` reference. Binding the pattern to a `const` keeps its editor highlighting.',
            examples: [
                {
                    code: 'melody: $`c4 e4 g4`.saw()',
                    title: 'A three-note line',
                },
                {
                    code: "const bass = $('c2 [c2 g2] c2 e2')\nline: $saw(bass).amp($perc(bass))",
                    title: 'A pattern bound to a const',
                },
            ],
            sections: [],
            summary: 'Mini-notation pattern',
        },
        params: { source: 'Mini-notation' },
        returns: { description: 'The pattern' },
    },
    {
        declarations: [
            '$arrange(...sections: [number, ScalePattern][]): ScalePattern',
            '$arrange(...sections: [number, Pattern][]): Pattern',
        ],
        docs: {
            categories: ['sequencer'],
            description:
                'Plays patterns back to back, each `[cycles, pattern]` section for its number of cycles, looping over their total. Each scale section keeps its own scale, so an arrangement can change key, and `.trans` moves each section within its scale.',
            examples: [
                {
                    code: "melody: $arrange(\n  [4, $`0 2 4 7`.scale('c(major)')],\n  [2, $`0 2 4`.scale('a(min)')],\n)\n  .trans('<0 1>')\n  .saw()",
                    title: 'Four bars of C, two of A minor',
                },
                {
                    code: "melody: $arrange(\n  [2, $`c4 e4 g4`],\n  [Infinity, $`0 2 4`.scale('f(lydian)')],\n).saw()",
                    title: 'An intro, then a loop forever',
                },
            ],
            sections: [
                {
                    body: 'Cycle counts are positive integers, except that the last section may be `Infinity` to loop once reached. A section after an `Infinity` one could never play, so it is rejected. An arrangement is a pattern like any other, so arrangements nest as sections.',
                    examples: [],
                    title: 'Section lengths',
                },
            ],
            summary: 'Patterns in sequence',
        },
        params: { sections: '`[cycles, pattern]` sections, in play order' },
        returns: {
            description:
                'The arrangement, a scale pattern when every section is one',
        },
    },
    {
        declarations: ['$setTempo(tempo: number): void'],
        docs: {
            categories: ['sequencer'],
            description: 'Sets the root clock’s tempo.',
            examples: [
                {
                    code: '$setTempo(140)\nhats: $tr808.hiHat($`c*8`)',
                    title: 'Hats at 140 BPM',
                },
            ],
            sections: [],
            summary: 'Set the tempo',
        },
        params: {
            tempo: { description: 'Beats per minute', meta: 'default 120' },
        },
    },
    {
        declarations: [
            '$setTimeSignature(numerator: number, denominator: number): void',
        ],
        docs: {
            categories: ['sequencer'],
            description:
                'Sets the root clock’s time signature. Both values are positive integers.',
            examples: [
                {
                    code: '$setTimeSignature(3, 4)\nkick: $tr808.kick($clock._barTrigger)',
                    title: 'A waltz downbeat',
                },
                {
                    code: '$setTimeSignature(7, 8)\nkick: $tr808.kick($clock._barTrigger)',
                    title: 'Seven-eight',
                },
            ],
            sections: [],
            summary: 'Set the time signature',
        },
        params: {
            denominator: 'The beat’s note value: 4 for quarters, 8 for eighths',
            numerator: { description: 'Beats per bar', meta: 'default 4' },
        },
    },
    {
        declarations: ['$setGroove(template: GrooveTemplate): void'],
        docs: {
            categories: ['sequencer'],
            description:
                'Sets the global groove: every `$cycle` and `$drumMap` starts its notes on it, keeping their lengths, unless given its own groove or playhead. `$clock._grooveOffset`, `$clock._grooveVelocity` and `$clock._grooveTrigger` read it directly.',
            examples: [
                {
                    code: '$setGroove($swing(0.6))\nhats: $tr808.hiHat($`c*16`)',
                    title: 'Sixteenth swing',
                },
                {
                    code: '$setGroove({\n  timing: [0, 0.2, 0, 0.15],\n  velocity: [1, 0.6, 0.8, 0.5],\n})\nhats: $tr808.hiHat($`c*16`)',
                    title: 'A hand-drawn groove',
                },
            ],
            sections: [],
            summary: 'Set the global groove',
        },
        params: {
            template:
                'Per-step timing offsets in grid steps and velocities from 0 to 1, e.g. from `$swing`',
        },
    },
    {
        declarations: [
            '$swing(ratio: number, config?: { grid?: number; humanize?: number }): GrooveTemplate',
        ],
        docs: {
            categories: ['sequencer'],
            description:
                'A swing groove: each pair of grid steps splits `ratio` : `1 - ratio`, so 0.5 is straight and 0.66 a triplet feel.',
            examples: [
                {
                    code: '$setGroove($swing(0.58))\nhats: $tr808.hiHat($`c*16`)',
                    title: 'Light sixteenth swing',
                },
                {
                    code: '$setGroove($swing(0.66, { grid: 8 }))\nhats: $tr808.hiHat($`c*8`)',
                    title: 'Triplet-feel eighths',
                },
                {
                    code: '$setGroove($swing(0.6, { humanize: 0.05 }))\nhats: $tr808.hiHat($`c*16`)',
                    title: 'Swing with a little looseness',
                },
            ],
            sections: [],
            summary: 'Swing groove',
        },
        params: {
            config: '`grid`: grid steps per cycle (default 16); `humanize`: the largest random timing offset per step, in grid steps (default 0)',
            ratio: 'Share of each pair given to its first step',
        },
        returns: { description: 'A groove for `$setGroove`' },
    },
    {
        declarations: ['$setMainGain(gain: LaneSum<Signal>): void'],
        docs: {
            categories: ['mixer'],
            description:
                'Sets the gain on the final mix, where each step of 1 is 12 dB.',
            examples: [
                {
                    code: "$setMainGain(3.5)\ntone: $saw('c3')",
                    title: '18 dB quieter',
                },
                {
                    code: "$setMainGain($sine('1hz').range(4, 5))\ntone: $saw('c3')",
                    title: 'Tremolo on the whole mix',
                },
            ],
            sections: [],
            summary: 'Set the main gain',
        },
        params: { gain: { description: 'The gain', meta: 'default 5, unity' } },
    },
    {
        declarations: [
            '$setEndOfChainCb(cb: (mixed: Collection) => Collection | CollectionWithRange): void',
        ],
        docs: {
            categories: ['mixer'],
            description:
                'Processes the final mix just before the main gain: `cb` receives the whole mix and returns what plays.',
            examples: [
                {
                    code: "$setEndOfChainCb((mix) => $lpf(mix, '2000hz'))\ntone: $saw('c3')",
                    title: 'A low-pass on everything',
                },
            ],
            sections: [],
            summary: 'Process the final mix',
        },
        params: { cb: 'Takes the mix and returns the processed signal' },
    },
    {
        declarations: [
            '$slider(label: string, value: number, min: number, max: number, group?: ControlGroup): CollectionWithRange',
            '$slider(label: string, value: HZ, min: HZ, max: HZ, group?: ControlGroup): CollectionWithRange',
            '$slider(label: string, value: Note, min: Note, max: Note, group?: ControlGroup): CollectionWithRange',
        ],
        docs: {
            categories: ['control'],
            description:
                'A slider in the Control panel, shown as soon as it is written. Dragging it rewrites `value` in the source and drives the sound live once the patch runs. Value, min and max share one unit: volts, Hz strings or notes, the last two travelling evenly in octaves and notes snapping to semitones.',
            examples: [
                {
                    code: "tone: $sine('c4').amp($slider('Volume', 0.5, 0, 1))",
                    title: 'A volume slider',
                },
                {
                    code: "tone: $sine($slider('Pitch', '440hz', '55hz', '1760hz'))",
                    title: 'A pitch slider in Hz',
                },
                {
                    code: "tone: $saw($slider('Root', 'c3', 'c2', 'c5'))",
                    title: 'A root note slider',
                },
            ],
            sections: [],
            summary: 'Slider control',
        },
        params: {
            group: CONTROL_GROUP,
            label: CONTROL_LABEL,
            max: 'The top of the slider, a literal in the same unit',
            min: 'The bottom of the slider, a literal in the same unit',
            value: 'The starting value, a literal',
        },
        returns: { description: 'The slider’s value', meta: 'min to max' },
    },
    {
        declarations: [
            '$btn(label: string, group?: ControlGroup): CollectionWithRange',
        ],
        docs: {
            categories: ['control'],
            description:
                'A momentary button in the Control panel: high while held. Chain `.hold()` for a trigger of fixed length per press.',
            examples: [
                {
                    code: "lead: $sine('c4').amp($adsr($btn('Play')))",
                    title: 'Play while held',
                },
                {
                    code: "lead: $sine('c4').amp($adsr($btn('Hit').hold(0.1)))",
                    title: 'A 100 ms hit per press',
                },
            ],
            sections: [],
            summary: 'Momentary button',
        },
        params: { group: CONTROL_GROUP, label: CONTROL_LABEL },
        returns: { description: 'The button’s state', meta: 'gate' },
    },
    {
        declarations: [
            '$toggleBtn(label: string, initial: boolean, group?: ControlGroup): CollectionWithRange',
        ],
        docs: {
            categories: ['control'],
            description:
                'A latching button in the Control panel. Each click flips it and rewrites `initial` in the source, so its state survives running the patch again.',
            examples: [
                {
                    code: "bass: $saw('c2').amp($toggleBtn('Drone', false))",
                    title: 'A drone switch',
                },
            ],
            sections: [],
            summary: 'Toggle button',
        },
        params: {
            group: CONTROL_GROUP,
            initial: 'Whether it starts on, a `true` or `false` literal',
            label: CONTROL_LABEL,
        },
        returns: { description: 'The button’s state', meta: 'gate' },
    },
    {
        declarations: [
            '$cGroup(label: string, params?: ControlGroupParams): ControlGroup',
        ],
        docs: {
            categories: ['control'],
            description:
                'A group of controls under a collapsible header in the Control panel. Controls given the group as their last argument appear in it, and labels need only be unique within their group. Clicking the header rewrites `collapsed` in the source, and groups nest through `params.group`.',
            examples: [
                {
                    code: "const voice = $cGroup('Voice')\ntone: $sine($slider('Pitch', 'c4', 'c2', 'c6', voice)).amp(\n  $adsr($btn('Play', voice)),\n)",
                    title: 'A voice’s controls together',
                },
                {
                    code: "const synth = $cGroup('Synth')\nconst amp = $cGroup('Amp', {\n  collapsed: true,\n  group: synth,\n})\ntone: $saw($slider('Root', 'c3', 'c2', 'c5', synth)).amp(\n  $slider('Level', 0.5, 0, 1, amp),\n)",
                    title: 'A collapsed group inside another',
                },
            ],
            sections: [],
            summary: 'Group of controls',
        },
        params: {
            label: 'Display label, a string literal unique among its siblings',
            params: '`collapsed`: whether it starts collapsed, a `true` or `false` literal; `group`: a group to nest in. An object literal',
        },
        returns: {
            description:
                'The group, to pass as a control’s `group`: the `$cGroup(...)` call itself or a const bound to it',
        },
    },
    {
        declarations: ['$group(): GroupSignal'],
        docs: {
            categories: ['mixer'],
            description:
                'A group: the sum, as one stereo lane, of every track whose label ends in `$` and this group’s track name. Those tracks play into it after their own pan, gain, mute and solo, and reach the output only through it. Chain effects on it and label the result: that track is the group.',
            examples: [
                {
                    code: 'kick$drums: $tr808.kick($`c ~ c ~`)\nhats$drums: $tr808.hiHat($`c*8`)\ndrums: $group().comp().track({ gain: 4 })',
                    title: 'Drums through one compressor',
                },
                {
                    code: "kick$drums: $tr808.kick($`c ~ c ~`)\ndrums$rhythm: $group()\nbass$rhythm: $saw('c2')\nrhythm$2: $group().lpf('c7')",
                    title: 'Nested groups on channels 2 and 3',
                },
            ],
            sections: [
                {
                    body: 'A group’s track plays to the main output, to the channels of its own `$N` or `$Nm` suffix, or into another group, so groups nest. A group has one track, a group track can send to buses, and a bus track cannot play into a group. The mixer shows a group with its tracks beside it, and can fold them away.',
                    examples: [],
                    title: 'Routing',
                },
            ],
            summary: 'Group of tracks',
        },
        params: {},
        returns: { description: 'The group’s signal, to chain on and label' },
    },
    {
        declarations: ['$bus(): BusSignal'],
        docs: {
            categories: ['mixer'],
            description:
                'A send-return bus: the mix, as one stereo lane, of every signal sent to it through the `sends` option of `.track()`. Chain effects on it and label the result to hear it: that track is the bus, and its name is what `sends` takes.',
            examples: [
                {
                    code: "fx: $bus().plate()\ntone: $saw('a').track({ sends: [fx, 3] })\nbass: $sine('a2').track({ sends: fx })",
                    title: 'A shared plate',
                },
                {
                    code: "verb: $bus().plate().track({ gain: 3 })\necho: $bus().delay(0.25)\nkeys: $saw('c3').track({\n  sends: [\n    [verb, 2],\n    [echo, 4],\n  ],\n})",
                    title: 'One track feeding two buses',
                },
            ],
            sections: [
                {
                    body: 'A bus has one track, and a bus track cannot send to another bus or be soloed: it keeps playing while other tracks are soloed, so a soloed track is heard with its sends. The mixer shows bus tracks on the right, just left of the main strip.',
                    examples: [],
                    title: 'Routing',
                },
            ],
            summary: 'Send-return bus',
        },
        params: {},
        returns: { description: 'The bus’s signal, to chain on and label' },
    },
    {
        declarations: ['$trackTap(name: string): TrackTap'],
        docs: {
            categories: ['mixer'],
            description:
                'Follows the track named `name` as it plays, through live mixer changes. A track label already binds its name to the same tap, so `$trackTap` is for a name held in a string; spaces and underscores match alike, and a name no track has is an error.',
            examples: [
                {
                    code: "bass: $saw('c2')\nconst tap = $trackTap('bass')\nlead: $sine('c5')\n  .gain(tap.level)\n  .track({ pan: tap.panning })\nbell: $sine('g5').gain(tap.active)",
                    title: 'A lead that follows the bass fader',
                },
            ],
            sections: [],
            summary: 'Follow a track',
        },
        params: { name: 'The track to follow' },
        returns: {
            description:
                'The track’s audio after pan, gain and mute or solo: one lane, stereo for a stereo track. A bus’s tap has its controls but no audio',
            members: {
                '.active': {
                    description:
                        'High while the track is audible, low while muted or silenced by another track’s solo',
                    meta: 'gate',
                },
                '.level': {
                    description:
                        'The fader’s gain, 5 unity, or the gain signal’s own range when it declares one',
                    meta: '0–10',
                },
                '.panning': {
                    description:
                        'The pan, 0 for a mono track, or the pan signal’s own range when it declares one',
                    meta: '−5–5',
                },
            },
        },
    },
];

/** Every global's reference entry, in `GLOBALS` order. */
export const GLOBAL_ENTRIES: ReferenceEntry[] = GLOBALS.map(
    functionToReferenceEntry,
);
