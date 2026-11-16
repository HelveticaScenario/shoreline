import { describe, expect, test } from 'vitest';

import ts from 'typescript';

import { parseDsl } from '../../../shared/dsl/ast/parse';
import {
    parseTrackLabel,
    type TrackState,
} from '../../../shared/dsl/trackLabels';
import {
    computeSetMainGainEdit,
    computeTrackOptionEdit,
    computeTrackStateEdit,
} from '../trackSourceEdit';

type Edit = { start: number; end: number; text: string } | null;

function applied(source: string, edit: Edit): string | null {
    return edit
        ? source.slice(0, edit.start) + edit.text + source.slice(edit.end)
        : null;
}

/** The offset of the last label in `source` naming track `name`. */
function anchorOf(source: string, name: string): number {
    const file = parseDsl(source);
    const label = file.statements
        .filter(ts.isLabeledStatement)
        .map((st) => st.label)
        .findLast((l) => {
            const track = parseTrackLabel(l.text);
            return !('error' in track) && track.name === name;
        });
    return label ? label.getStart(file) : -1;
}

const STATES: Record<string, TrackState> = {
    both: { mute: true, solo: true },
    mute: { mute: true, solo: false },
    none: { mute: false, solo: false },
    solo: { mute: false, solo: true },
};

const state = (
    source: string,
    name: string,
    next: 'mute' | 'solo' | 'both' | null,
) =>
    applied(
        source,
        computeTrackStateEdit(
            source,
            anchorOf(source, name),
            name,
            STATES[next ?? 'none'],
        ),
    );

const option = (
    source: string,
    name: string,
    prop: 'gain' | 'pan',
    value: number | null,
    fromTrack = false,
) =>
    applied(
        source,
        computeTrackOptionEdit(
            source,
            anchorOf(source, name),
            name,
            prop,
            value,
            fromTrack,
        ),
    );

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

    test('muted and soloed writes S__, and each clears on its own', () => {
        expect(state(`_lead: $saw('c3')`, 'lead', 'both')).toBe(
            `S__lead: $saw('c3')`,
        );
        expect(state(`_S_lead: $saw('c3')`, 'lead', 'mute')).toBe(
            `_lead: $saw('c3')`,
        );
        expect(state(`S__lead: $saw('c3')`, 'lead', 'solo')).toBe(
            `S_lead: $saw('c3')`,
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

    test('a track with a scope but no .track() gains one after the scope', () => {
        expect(option(`lead: $saw('c3').scope()`, 'lead', 'gain', 3)).toBe(
            `lead: $saw('c3').scope().track({ gain: 3 })`,
        );
    });

    test('a low-precedence expression is wrapped before gaining .track()', () => {
        expect(
            option(`lead: up ? $saw('c3') : $sine('c3')`, 'lead', 'gain', 3),
        ).toBe(`lead: (up ? $saw('c3') : $sine('c3')).track({ gain: 3 })`);
    });

    test('a track made elsewhere gains no .track()', () => {
        expect(option(`lead: mk('c3')`, 'lead', 'gain', 3, true)).toBeNull();
    });

    test('the edit lands on the anchored label, not a newer one of its name', () => {
        const source = `lead: $sine('c2')\nlead: $saw('c3')`;
        const anchor = source.lastIndexOf('lead');
        expect(
            applied(
                source,
                computeTrackStateEdit(source, anchor, 'lead', STATES.mute),
            ),
        ).toBe(`lead: $sine('c2')\n_lead: $saw('c3')`);
    });

    test('an anchor no longer on the track is no edit', () => {
        expect(
            computeTrackStateEdit(`lead: $saw('c3')`, 3, 'lead', STATES.mute),
        ).toBeNull();
        expect(
            computeTrackStateEdit(`bass: $saw('c3')`, 0, 'lead', STATES.mute),
        ).toBeNull();
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
        expect(option(`verb: $bus().plate()`, 'verb', 'gain', 3)).toBe(
            `verb: $bus().plate().track({ gain: 3 })`,
        );
    });

    test('a name bound to a .track() elsewhere is not edited', () => {
        const source = `const t = $saw('c3').track()\nlead: t`;
        expect(option(source, 'lead', 'gain', 3, true)).toBeNull();
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

describe('computeSetMainGainEdit — master fader', () => {
    const master = (source: string, value: number) =>
        applied(source, computeSetMainGainEdit(source, value));

    test('updates an existing numeric call', () => {
        expect(master(`$setMainGain(2.5)\nlead: $sine('c4')`, 4)).toBe(
            `$setMainGain(4)\nlead: $sine('c4')`,
        );
    });

    test('appends a call when none exists', () => {
        expect(master(`lead: $sine('c4')`, 3.2)).toBe(
            `lead: $sine('c4')\n$setMainGain(3.2)\n`,
        );
    });

    test('a signal-driven main gain is not edited', () => {
        expect(
            master(`$setMainGain($sine('1hz'))\nlead: $sine('c4')`, 4),
        ).toBeNull();
    });

    test('a commented-out call does not count', () => {
        expect(master(`// $setMainGain(1)\nlead: $sine('c4')\n`, 4)).toBe(
            `// $setMainGain(1)\nlead: $sine('c4')\n$setMainGain(4)\n`,
        );
    });
});
