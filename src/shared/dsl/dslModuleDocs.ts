/**
 * Modules the DSL builds from other modules: their TypeScript declarations
 * and docs, which the generated Monaco typings and the Docs panel's Modules
 * page both read.
 */
import type { ModuleSchema } from '@modular/core';

import type { FunctionDoc } from './referenceDocs';
import { superDefaults } from './superDefaults';

const DELAY: FunctionDoc = {
    chainable: true,
    declarations: [
        '$delay(input: Collection | CollectionWithRange, time: Poly<Signal>, opts?: { feedback?: Poly<Signal>; feedbackCb?: (mixed: Collection) => Collection | CollectionWithRange; maxTime?: number }): Collection',
    ],
    docs: {
        categories: ['effect'],
        description:
            'A delay line whose echoes feed back into it. It returns only the echoes, so mix the dry signal back in for a classic echo.',
        examples: [
            {
                code: "const src = $saw('c3')\necho: $mix([src, $delay(src, 0.25, { feedback: 3 })])",
                title: 'Echoes beside the dry signal',
            },
            {
                code: "dark: $saw('c3').delay(0.5, {\n  feedbackCb: (m) => m.lpf('800hz'),\n})",
                title: 'Darker with every repeat',
            },
        ],
        sections: [],
        summary: 'Delay with feedback',
    },
    params: {
        input: 'The signal to echo',
        opts: '`feedback`: how much of each echo returns, clamped to 0–5 with 5 unity (default 2.5; at 0 you still hear one echo); `feedbackCb`: processes the input and returning echoes before each pass, so a filter there darkens every repeat; `maxTime`: the longest `time` in seconds, which sizes the buffer (default 5)',
        time: 'Seconds from the input to its first echo, up to `maxTime`',
    },
    returns: { description: 'The echoes, without the dry signal' },
};

/** Every DSL-built module, in name order. `$super` states its defaults, read
 *  from the modules it stands in for, so it is listed only beside them. */
export function dslModules(schemas: readonly ModuleSchema[]): FunctionDoc[] {
    const docs = [DELAY];
    const defaults = superDefaults(schemas);
    if (defaults) {
        const { unison, detune, width } = defaults;
        docs.push({
            declarations: [
                '$super(freq: Poly<Signal>, cb: (freq: UnisonOutputs, options: { phaseOffset: CollectionWithRange }) => Collection | CollectionWithRange, options?: { unison?: number; detune?: Poly<Signal>; width?: Poly<Signal> }): Collection',
            ],
            docs: {
                categories: ['oscillator'],
                description:
                    'A detuned, stereo-spread unison stack of any voice: the copies are folded into one stereo lane per note the way `$supersaw` spreads its saws. Pass `phaseOffset` on to the oscillator so the copies never all line up.',
                examples: [
                    {
                        code: "lead: $super('c3', (freq, { phaseOffset }) =>\n  $saw(freq, { phaseOffset }),\n)",
                        title: 'A super saw',
                    },
                    {
                        code: "pad: $super(\n  'c3:m9',\n  (freq, { phaseOffset }) =>\n    $pulse(freq, { phaseOffset }).lpf('c6'),\n  { unison: 3, detune: 0.3 },\n)",
                        title: 'A filtered pulse chord',
                    },
                ],
                sections: [],
                summary: 'Unison stack of any voice',
            },
            params: {
                cb: "Builds one voice from the copies' pitches",
                freq: "The note's pitch",
                options: `\`unison\`: copies per note (default ${unison}); \`detune\`: semitones between the lowest and highest copy (default ${detune}); \`width\`: how far the copies fan out, 0 centred to 5 hard left and right (default ${width})`,
            },
            returns: { description: 'One stereo lane per note' },
        });
    }
    return docs;
}
