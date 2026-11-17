import { expect, test } from 'vitest';

import { trackGates } from '../trackGates';

const track = (
    key: string,
    state: 'mute' | 'solo' | null = null,
    group: string | null = null,
) => ({ group, key, mute: state === 'mute', solo: state === 'solo' });

const open = (tracks: ReturnType<typeof track>[]) =>
    [...trackGates(tracks)].filter(([, gate]) => gate).map(([key]) => key);

test('with no solo, a track plays unless muted', () => {
    expect(open([track('a'), track('b', 'mute')])).toEqual(['a']);
});

test('a soloed track in nested groups opens every group around it', () => {
    expect(
        open([
            track('kick', 'solo', 'drums'),
            track('snare', null, 'drums'),
            track('drums', null, 'rhythm'),
            track('bass', null, 'rhythm'),
            track('rhythm'),
            track('lead'),
        ]),
    ).toEqual(['kick', 'drums', 'rhythm']);
});

test('a soloed outer group plays everything inside it', () => {
    expect(
        open([
            track('kick', null, 'drums'),
            track('drums', null, 'rhythm'),
            track('rhythm', 'solo'),
            track('lead'),
        ]),
    ).toEqual(['kick', 'drums', 'rhythm']);
});

test('a group that plays into itself does not hang', () => {
    expect(open([track('a', 'solo', 'b'), track('b', null, 'a')])).toEqual([
        'a',
        'b',
    ]);
});

test("a bus plays through other tracks' solos, unless muted", () => {
    const bus = (key: string, mute: boolean) => ({
        group: null,
        isBus: true,
        key,
        mute,
        solo: false,
    });
    expect(
        open([track('lead', 'solo'), track('bass'), bus('verb', false)]),
    ).toEqual(['lead', 'verb']);
    expect(open([track('lead', 'solo'), bus('verb', true)])).toEqual(['lead']);
});
