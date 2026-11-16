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
        ['lead', { channel: 0, mono: false, name: 'lead', state: null }],
        ['_lead', { channel: 0, mono: false, name: 'lead', state: 'mute' }],
        ['s_lead', { channel: 0, mono: false, name: 'lead', state: 'solo' }],
        ['S_lead', { channel: 0, mono: false, name: 'lead', state: 'solo' }],
        ['lead$2', { channel: 2, mono: false, name: 'lead', state: null }],
        ['lead$12m', { channel: 12, mono: true, name: 'lead', state: null }],
        [
            '_bass_line$0m',
            { channel: 0, mono: true, name: 'bass_line', state: 'mute' },
        ],
        ['snare', { channel: 0, mono: false, name: 'snare', state: null }],
        ['s', { channel: 0, mono: false, name: 's', state: null }],
        [
            'a$b',
            { channel: 0, group: 'b', mono: false, name: 'a', state: null },
        ],
        [
            '_kick$drums',
            { group: 'drums', groupStart: 6, name: 'kick', state: 'mute' },
        ],
        ['lead$2', { group: null }],
    ])('%s', (label, expected) => {
        expect(parseTrackLabel(label)).toMatchObject(expected);
    });

    test('the mixer shows underscores as spaces', () => {
        expect(parseTrackLabel('S_bass_line$2')).toMatchObject({
            displayName: 'bass line',
        });
    });

    test.each(['__lead', 's__lead', '_', 'S_', '$2', 'x$63', 'x$64m'])(
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

test('withTrackState rewrites only the prefix', () => {
    expect(withTrackState('lead$2', 'mute')).toBe('_lead$2');
    expect(withTrackState('_lead$2', 'solo')).toBe('S_lead$2');
    expect(withTrackState('s_lead$2m', null)).toBe('lead$2m');
    expect(withTrackState('snare', 'solo')).toBe('S_snare');
});

test.each(['a$b$c', 'a$_b', 'a$', 'a$2x'])(
    '%s has no valid suffix',
    (label) => {
        expect(parseTrackLabel(label)).toHaveProperty('error');
    },
);

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
