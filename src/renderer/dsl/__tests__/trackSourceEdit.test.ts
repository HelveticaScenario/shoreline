import { describe, expect, test } from 'vitest';

import type { TrackState } from '../../../shared/dsl/trackLabels';
import {
    computeSetMasterGainEdit,
    computeTrackOptionEdit,
    computeTrackStateEdit,
} from '../trackSourceEdit';

type Edit = { start: number; end: number; text: string } | null;

function applied(source: string, edit: Edit): string | null {
    return edit
        ? source.slice(0, edit.start) + edit.text + source.slice(edit.end)
        : null;
}

const state = (source: string, name: string, next: TrackState) =>
    applied(source, computeTrackStateEdit(source, name, next));

const option = (
    source: string,
    name: string,
    prop: 'gain' | 'pan',
    value: number | null,
) => applied(source, computeTrackOptionEdit(source, name, prop, value));

describe('computeTrackStateEdit — the label prefix', () => {
    test('mute writes _', () => {
        expect(state(`lead: $saw('c3')`, 'lead', 'mute')).toBe(
            `_lead: $saw('c3')`,
        );
    });

    test('solo writes S_', () => {
        expect(state(`lead$2: $saw('c3')`, 'lead', 'solo')).toBe(
            `S_lead$2: $saw('c3')`,
        );
    });

    test('solo replaces a mute and a mute replaces a solo', () => {
        expect(state(`_lead: $saw('c3')`, 'lead', 'solo')).toBe(
            `S_lead: $saw('c3')`,
        );
        expect(state(`s_lead$0m: $saw('c3')`, 'lead', 'mute')).toBe(
            `_lead$0m: $saw('c3')`,
        );
    });

    test('neither removes the prefix', () => {
        expect(state(`S_bass_line: $saw('c2')`, 'bass_line', null)).toBe(
            `bass_line: $saw('c2')`,
        );
    });

    test('a name starting with s is not a solo prefix', () => {
        expect(state(`snare: $noise()`, 'snare', 'mute')).toBe(
            `_snare: $noise()`,
        );
    });

    test('finds the track by name among others', () => {
        const source = `kick: $sine('c1')\nbass: $saw('c2')`;
        expect(state(source, 'bass', 'mute')).toBe(
            `kick: $sine('c1')\n_bass: $saw('c2')`,
        );
    });

    test('no edit for a missing name or an unchanged state', () => {
        expect(state(`lead: $saw('c3')`, 'bass', 'mute')).toBeNull();
        expect(state(`_lead: $saw('c3')`, 'lead', 'mute')).toBeNull();
    });
});

describe('computeTrackOptionEdit — .track options', () => {
    test('a track without .track() gains one', () => {
        expect(option(`lead: $saw('c3')`, 'lead', 'gain', 3)).toBe(
            `lead: $saw('c3').track({ gain: 3 })`,
        );
    });

    test('an empty .track() gains its options', () => {
        expect(option(`lead: $saw('c3').track()`, 'lead', 'pan', -2)).toBe(
            `lead: $saw('c3').track({ pan: -2 })`,
        );
    });

    test('an existing option is rewritten', () => {
        expect(
            option(
                `lead: $saw('c3').track({ gain: 2, pan: 1 })`,
                'lead',
                'pan',
                -1.5,
            ),
        ).toBe(`lead: $saw('c3').track({ gain: 2, pan: -1.5 })`);
    });

    test('a new option goes after the others', () => {
        expect(
            option(
                `lead: $saw('c3').track({ sends: verb })`,
                'lead',
                'gain',
                4,
            ),
        ).toBe(`lead: $saw('c3').track({ sends: verb, gain: 4 })`);
    });

    test('.track() before a scope is found through it', () => {
        expect(
            option(
                `lead: $saw('c3').track({ gain: 2 }).scope()`,
                'lead',
                'gain',
                3,
            ),
        ).toBe(`lead: $saw('c3').track({ gain: 3 }).scope()`);
    });

    test('a track with a scope but no .track() gains one before the scope', () => {
        expect(option(`lead: $saw('c3').scope()`, 'lead', 'gain', 3)).toBe(
            `lead: $saw('c3').track({ gain: 3 }).scope()`,
        );
    });

    test('removing the last option drops .track()', () => {
        expect(
            option(`lead: $saw('c3').track({ gain: 3 })`, 'lead', 'gain', null),
        ).toBe(`lead: $saw('c3')`);
    });

    test('removing the last option keeps .track() a scope chains on', () => {
        expect(
            option(
                `lead: $saw('c3').track({ gain: 3 }).scope()`,
                'lead',
                'gain',
                null,
            ),
        ).toBe(`lead: $saw('c3').track().scope()`);
    });

    test('removing one of several options keeps the rest', () => {
        expect(
            option(
                `lead: $saw('c3').track({ gain: 3, pan: 1 })`,
                'lead',
                'pan',
                null,
            ),
        ).toBe(`lead: $saw('c3').track({ gain: 3 })`);
    });

    test('removing an option that is not there is no edit', () => {
        expect(option(`lead: $saw('c3')`, 'lead', 'pan', null)).toBeNull();
    });

    test('a signal-valued option is not rewritten', () => {
        expect(
            option(
                `lead: $saw('c3').track({ pan: $sine('1hz') })`,
                'lead',
                'pan',
                2,
            ),
        ).toBeNull();
    });

    test('a bus gains .track() like a signal', () => {
        expect(option(`verb: $bus((m) => m.plate())`, 'verb', 'gain', 3)).toBe(
            `verb: $bus((m) => m.plate()).track({ gain: 3 })`,
        );
    });

    test('a name bound to a .track() elsewhere is not edited', () => {
        const source = `const t = $saw('c3').track()\nlead: t`;
        expect(option(source, 'lead', 'gain', 3)).toBeNull();
    });

    test('a name bound to a signal gains .track()', () => {
        const source = `const v = $saw('c3')\nlead: v`;
        expect(option(source, 'lead', 'gain', 3)).toBe(
            `const v = $saw('c3')\nlead: v.track({ gain: 3 })`,
        );
    });

    test('a property on its own line goes with its line and its comment', () => {
        const source = `lead: $saw('c3').track({
  gain: 3, // loud
  pan: 1,
})`;
        expect(option(source, 'lead', 'gain', null))
            .toBe(`lead: $saw('c3').track({
  pan: 1,
})`);
    });

    test('values are written to four significant digits', () => {
        expect(option(`lead: $saw('c3')`, 'lead', 'gain', 1.234567)).toBe(
            `lead: $saw('c3').track({ gain: 1.235 })`,
        );
    });
});

describe('computeSetMasterGainEdit — master fader', () => {
    const master = (source: string, value: number) =>
        applied(source, computeSetMasterGainEdit(source, value));

    test('updates an existing numeric call', () => {
        expect(master(`$setMasterGain(2.5)\nlead: $sine('c4')`, 4)).toBe(
            `$setMasterGain(4)\nlead: $sine('c4')`,
        );
    });

    test('appends a call when none exists', () => {
        expect(master(`lead: $sine('c4')`, 3.2)).toBe(
            `lead: $sine('c4')\n$setMasterGain(3.2)\n`,
        );
    });

    test('a signal-driven master gain is not edited', () => {
        expect(
            master(`$setMasterGain($sine('1hz'))\nlead: $sine('c4')`, 4),
        ).toBeNull();
    });

    test('a commented-out call does not count', () => {
        expect(master(`// $setMasterGain(1)\nlead: $sine('c4')\n`, 4)).toBe(
            `// $setMasterGain(1)\nlead: $sine('c4')\n$setMasterGain(4)\n`,
        );
    });
});
