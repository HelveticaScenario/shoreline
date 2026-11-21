import { describe, expect, test } from 'vitest';

import ts from 'typescript';

import { parseDsl } from '../../../shared/dsl/ast/parse';
import { parseTrackLabel } from '../../../shared/dsl/trackLabels';
import { computeTrackSendEdit } from '../trackSendEdit';

const BUSES = new Set(['a', 'b', 'c']);

/** `source` with track `lead`'s send to `bus` set to `level`, or null when
 *  the dial edits nothing. */
function send(
    source: string,
    bus: string,
    level: number | null,
    fromTrack = false,
): string | null {
    const file = parseDsl(source);
    const label = file.statements
        .filter(ts.isLabeledStatement)
        .map((st) => st.label)
        .find((l) => {
            const track = parseTrackLabel(l.text);
            return !('error' in track) && track.name === 'lead';
        })!;
    const edit = computeTrackSendEdit(
        source,
        label.getStart(file),
        'lead',
        bus,
        level,
        fromTrack,
        BUSES,
    );
    return edit
        ? source.slice(0, edit.start) + edit.text + source.slice(edit.end)
        : null;
}

describe('setting a send', () => {
    test.each([
        // Added to a track with no sends.
        [
            `lead: $saw('c3')`,
            'a',
            2,
            `lead: $saw('c3').track({ sends: [a, 2] })`,
        ],
        [
            `lead: $saw('c3').track()`,
            'a',
            2,
            `lead: $saw('c3').track({ sends: [a, 2] })`,
        ],
        [
            `lead: $saw('c3').track({ gain: 3 })`,
            'a',
            2,
            `lead: $saw('c3').track({ gain: 3, sends: [a, 2] })`,
        ],
        // Added beside existing sends.
        [
            `lead: $saw('c3').track({ sends: a })`,
            'b',
            2,
            `lead: $saw('c3').track({ sends: [a, [b, 2]] })`,
        ],
        [
            `lead: $saw('c3').track({ sends: [a, 3] })`,
            'b',
            2,
            `lead: $saw('c3').track({ sends: [[a, 3], [b, 2]] })`,
        ],
        [
            `lead: $saw('c3').track({ sends: [a, b] })`,
            'c',
            2,
            `lead: $saw('c3').track({ sends: [a, b, [c, 2]] })`,
        ],
        [
            `lead: $saw('c3').track({ sends: [] })`,
            'a',
            2,
            `lead: $saw('c3').track({ sends: [[a, 2]] })`,
        ],
        [
            `lead: $saw('c3').track({ sends: [/* fx */] })`,
            'a',
            2,
            `lead: $saw('c3').track({ sends: [/* fx */[a, 2]] })`,
        ],
        // Changed in place.
        [
            `lead: $saw('c3').track({ sends: [a, 3] })`,
            'a',
            1.5,
            `lead: $saw('c3').track({ sends: [a, 1.5] })`,
        ],
        [
            `lead: $saw('c3').track({ sends: [a, [b, 4]] })`,
            'b',
            1,
            `lead: $saw('c3').track({ sends: [a, [b, 1]] })`,
        ],
        [
            `lead: $saw('c3').track({ sends: a })`,
            'a',
            2,
            `lead: $saw('c3').track({ sends: [a, 2] })`,
        ],
        [
            `lead: $saw('c3').track({ sends: [a, b] })`,
            'b',
            2,
            `lead: $saw('c3').track({ sends: [a, [b, 2]] })`,
        ],
    ])('%s: %s at %s', (source, bus, level, expected) => {
        expect(send(source, bus, level)).toBe(expected);
    });

    test.each([
        // A bus's name alone already plays at full level.
        [`lead: $saw('c3').track({ sends: a })`, 'a', 5],
        // A signal level is the signal's to set.
        [`lead: $saw('c3').track({ sends: [a, $sine(1)] })`, 'a', 2],
        // Two sends to one bus have no one level to set.
        [`lead: $saw('c3').track({ sends: [a, [a, 2]] })`, 'a', 2],
        // A value the dials cannot read.
        [`lead: $saw('c3').track({ sends: mySends })`, 'a', 2],
        [`lead: $saw('c3').track(opts)`, 'a', 2],
        // A name, spread or array second may hold another send, so the text
        // alone cannot tell a pair from a list.
        [`lead: $saw('c3').track({ sends: [a, ...more] })`, 'a', 0],
        [`lead: $saw('c3').track({ sends: [a, fx] })`, 'c', 2],
        [`lead: $saw('c3').track({ sends: [a, level] })`, 'a', 2],
        // A spread may hold sends an edit would override.
        [`lead: $saw('c3').track({ ...opts })`, 'b', 2],
        [`lead: $saw('c3').track({ sends: a, ...opts })`, 'b', 2],
    ])('%s: %s at %s edits nothing', (source, bus, level) => {
        expect(send(source, bus, level === 0 ? null : level)).toBeNull();
    });

    test('a track made elsewhere is not appended to', () => {
        expect(send(`lead: make()`, 'a', 2, true)).toBeNull();
    });
});

describe('removing a send', () => {
    test.each([
        [
            `lead: $saw('c3').track({ sends: [a, [b, 2]] })`,
            'a',
            `lead: $saw('c3').track({ sends: [[b, 2]] })`,
        ],
        [
            `lead: $saw('c3').track({ sends: [a, [b, 2]] })`,
            'b',
            `lead: $saw('c3').track({ sends: [a] })`,
        ],
        // The last send takes the option with it.
        [
            `lead: $saw('c3').track({ gain: 3, sends: [a, 2] })`,
            'a',
            `lead: $saw('c3').track({ gain: 3 })`,
        ],
        [
            `lead: $saw('c3').track({ gain: 3, sends: [[a, 2]] })`,
            'a',
            `lead: $saw('c3').track({ gain: 3 })`,
        ],
        // When that option is the track's last, the `.track()` goes too.
        [`lead: $saw('c3').track({ sends: a })`, 'a', `lead: $saw('c3')`],
        [`lead: $saw('c3').track({ sends: [a, 0] })`, 'a', `lead: $saw('c3')`],
    ])('%s without %s', (source, bus, expected) => {
        expect(send(source, bus, null)).toBe(expected);
    });

    test('a send that is not there removes nothing', () => {
        expect(send(`lead: $saw('c3').track({ sends: a })`, 'b', null)).toBe(
            null,
        );
        expect(send(`lead: $saw('c3')`, 'a', null)).toBeNull();
    });
});
