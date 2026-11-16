import ts from 'typescript';
import { describe, expect, test } from 'vitest';

import { parseDsl } from '../ast/parse';
import {
    analyzeTrackLabels,
    parseTrackLabel,
    trackNameKey,
    withTrackState,
} from '../trackLabels';

describe('parseTrackLabel', () => {
    test.each([
        [
            'lead',
            {
                channel: 0,
                mono: false,
                name: 'lead',
                state: { mute: false, solo: false },
            },
        ],
        [
            '_lead',
            {
                channel: 0,
                mono: false,
                name: 'lead',
                state: { mute: true, solo: false },
            },
        ],
        [
            's_lead',
            {
                channel: 0,
                mono: false,
                name: 'lead',
                state: { mute: false, solo: true },
            },
        ],
        [
            'S_lead',
            {
                channel: 0,
                mono: false,
                name: 'lead',
                state: { mute: false, solo: true },
            },
        ],
        [
            'lead$2',
            {
                channel: 2,
                mono: false,
                name: 'lead',
                state: { mute: false, solo: false },
            },
        ],
        [
            'lead$12m',
            {
                channel: 12,
                mono: true,
                name: 'lead',
                state: { mute: false, solo: false },
            },
        ],
        [
            '_bass_line$0m',
            {
                channel: 0,
                mono: true,
                name: 'bass_line',
                state: { mute: true, solo: false },
            },
        ],
        [
            'snare',
            {
                channel: 0,
                mono: false,
                name: 'snare',
                state: { mute: false, solo: false },
            },
        ],
        [
            's',
            {
                channel: 0,
                mono: false,
                name: 's',
                state: { mute: false, solo: false },
            },
        ],
        [
            'a$b',
            {
                channel: 0,
                mono: false,
                name: 'a$b',
                state: { mute: false, solo: false },
            },
        ],
    ])('%s', (label, expected) => {
        expect(parseTrackLabel(label)).toMatchObject(expected);
    });

    test('the mixer shows underscores as spaces', () => {
        expect(parseTrackLabel('S_bass_line$2')).toMatchObject({
            displayName: 'bass line',
        });
    });

    test.each(['__lead', '_', 'S_', '$2', 'x$63', 'x$64m'])(
        '%s is not a track label',
        (label) => {
            expect(parseTrackLabel(label)).toHaveProperty('error');
        },
    );

    test('the highest channels', () => {
        expect(parseTrackLabel('x$62')).toMatchObject({ channel: 62 });
        expect(parseTrackLabel('x$63m')).toMatchObject({ channel: 63 });
    });
});

test.each([
    ['S__lead', 'lead'],
    ['s__lead', 'lead'],
    ['_S_lead$2', 'lead'],
    ['_s_bass_line', 'bass_line'],
])('%s is muted and soloed', (label, name) => {
    expect(parseTrackLabel(label)).toMatchObject({
        name,
        prefixLength: 3,
        state: { mute: true, solo: true },
    });
});

test.each(['_1abc', 'S_1abc', '_9', 'S___x', '_S__x', 'S_s_x', '_S_s_x'])(
    '%s: no track name starts with a digit, _ or a solo prefix',
    (label) => {
        expect(parseTrackLabel(label)).toHaveProperty('error');
    },
);

test('withTrackState writes S__ for a muted and soloed track', () => {
    expect(withTrackState('lead$2', { mute: true, solo: true })).toBe(
        'S__lead$2',
    );
    expect(withTrackState('_S_lead', { mute: true, solo: false })).toBe(
        '_lead',
    );
    expect(withTrackState('S__lead', { mute: false, solo: false })).toBe(
        'lead',
    );
});

test('withTrackState rewrites only the prefix', () => {
    expect(withTrackState('lead$2', { mute: true, solo: false })).toBe(
        '_lead$2',
    );
    expect(withTrackState('_lead$2', { mute: false, solo: true })).toBe(
        'S_lead$2',
    );
    expect(withTrackState('s_lead$2m', { mute: false, solo: false })).toBe(
        'lead$2m',
    );
    expect(withTrackState('snare', { mute: false, solo: true })).toBe(
        'S_snare',
    );
});

test('trackNameKey matches spaces and underscores alike', () => {
    expect(trackNameKey('bass line')).toBe(trackNameKey('bass_line'));
});

describe('analyzeTrackLabels', () => {
    const analyze = (source: string) =>
        analyzeTrackLabels(ts, parseDsl(source));

    test('reads each top-level track with its spans', () => {
        const source = `kick: $sine('c1')\n_bass$2 : $saw('c2')`;
        const { tracks, errors } = analyze(source);
        expect(errors).toEqual([]);
        expect(tracks.map((t) => t.track.name)).toEqual(['kick', 'bass']);
        const bass = tracks[1];
        expect(source.slice(bass.labelStart, bass.labelEnd)).toBe('_bass$2');
        expect(source.slice(bass.labelStart, bass.colonEnd)).toBe('_bass$2 :');
        expect(bass.expression.getText()).toBe(`$saw('c2')`);
    });

    test('a nested label is an error at the label', () => {
        const source = `for (;;) { outer: break }`;
        const { errors } = analyze(source);
        expect(errors).toHaveLength(1);
        expect(source.slice(errors[0].start, errors[0].end)).toBe('outer');
        expect(errors[0].message).toMatch(/top level/);
    });

    test('a repeated name is an error on the second label', () => {
        const { tracks, errors } = analyze(`a: 1\n_a$2: 2`);
        expect(tracks).toHaveLength(1);
        expect(errors[0].message).toMatch(/already a track named a/);
    });

    test('a name a declaration binds is an error', () => {
        expect(analyze(`function a() {}\na: 1`).errors).toHaveLength(1);
        expect(analyze(`const [a] = [1]\na: 1`).errors).toHaveLength(1);
    });

    test('an unlabelled statement is no track', () => {
        expect(analyze(`$sine('c4')`)).toEqual({ errors: [], tracks: [] });
    });
});
