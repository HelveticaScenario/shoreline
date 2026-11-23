import { describe, expect, test } from 'vitest';

import { mixerOn, track } from './mixerHarness';

const SOURCE = "lead: $sine('c4')\nbass: $saw('c2')\n";
const tracks = () => [track('lead'), track('bass')];

describe('a plain click', () => {
    test('mute closes the gate, prefixes the label and shows muted', () => {
        const { mixer, calls, text } = mixerOn(SOURCE, tracks());
        mixer.tracks.toggle('lead', 'mute', false);
        expect(calls).toEqual([
            ['mute-lead', '$signal', { max: 5, min: 0, value: 0 }],
        ]);
        expect(text()).toBe("_lead: $sine('c4')\nbass: $saw('c2')\n");
        expect(mixer.store.output('lead')?.mute).toBe(true);
        expect(mixer.store.ghost('lead')).toBeUndefined();
    });

    test('unmute opens the gate again', () => {
        const { mixer, calls, text } = mixerOn(SOURCE, tracks());
        mixer.tracks.toggle('lead', 'mute', false);
        mixer.tracks.toggle('lead', 'mute', false);
        expect(calls.at(-1)).toEqual([
            'mute-lead',
            '$signal',
            { max: 5, min: 0, value: 5 },
        ]);
        expect(text()).toBe(SOURCE);
    });

    test('solo closes only the gates of the other tracks', () => {
        const { mixer, calls, text } = mixerOn(SOURCE, tracks());
        mixer.tracks.toggle('bass', 'solo', false);
        expect(calls).toEqual([
            ['mute-lead', '$signal', { max: 5, min: 0, value: 0 }],
        ]);
        expect(text()).toBe("lead: $sine('c4')\nS_bass: $saw('c2')\n");
        expect(mixer.store.output('bass')?.solo).toBe(true);
    });

    test('a bus has no solo', () => {
        const { mixer, calls, text } = mixerOn(SOURCE, [
            track('lead'),
            track('bass', { isBus: true }),
        ]);
        mixer.tracks.toggle('bass', 'solo', false);
        expect(calls).toEqual([]);
        expect(text()).toBe(SOURCE);
    });

    test('reaches the audio alone while the editor shows another buffer', () => {
        const { mixer, calls, text } = mixerOn(SOURCE, tracks());
        mixer.edits.setVisibleBuffer('another-buffer');
        mixer.tracks.toggle('lead', 'mute', false);
        expect(calls).toHaveLength(1);
        expect(text()).toBe(SOURCE);
        expect(mixer.store.output('lead')?.mute).toBe(true);
    });
});

describe('a code-only click', () => {
    test('edits the source and ghosts, leaving the audio', () => {
        const { mixer, calls, text } = mixerOn(SOURCE, tracks());
        mixer.tracks.toggle('lead', 'mute', true);
        expect(calls).toEqual([]);
        expect(text()).toBe("_lead: $sine('c4')\nbass: $saw('c2')\n");
        expect(mixer.store.output('lead')?.mute).toBe(false);
        expect(mixer.store.ghost('lead')).toEqual({ mute: true });
    });

    test('a second one brings the code back to the audio', () => {
        const { mixer, text } = mixerOn(SOURCE, tracks());
        mixer.tracks.toggle('lead', 'mute', true);
        mixer.tracks.toggle('lead', 'mute', true);
        expect(text()).toBe(SOURCE);
        expect(mixer.store.ghost('lead')).toBeUndefined();
    });

    test('leaves no ghost when the source cannot be edited', () => {
        const { mixer } = mixerOn(SOURCE, tracks());
        mixer.edits.setVisibleBuffer('another-buffer');
        mixer.tracks.toggle('lead', 'mute', true);
        expect(mixer.store.ghost('lead')).toBeUndefined();
    });

    test('a plain click keeps the other control as the code has it', () => {
        const { mixer, text } = mixerOn(SOURCE, tracks());
        mixer.tracks.toggle('bass', 'solo', true);
        mixer.tracks.toggle('bass', 'mute', false);
        expect(text()).toBe("lead: $sine('c4')\nS__bass: $saw('c2')\n");
        expect(mixer.store.ghost('bass')).toEqual({ solo: true });
    });
});
