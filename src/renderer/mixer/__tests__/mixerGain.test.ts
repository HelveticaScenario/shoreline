import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest';

import { mixerOn, track } from './mixerHarness';

const SOURCE = "lead: $sine('c4')\n";

// The source-write throttle runs on window timers.
beforeEach(() => {
    vi.stubGlobal('window', globalThis);
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout', 'performance'] });
    vi.advanceTimersByTime(1000);
});
afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
});

describe('a fader drag', () => {
    test('snaps to half a dB, drives the gain and writes the option', () => {
        const { mixer, calls, text } = mixerOn(SOURCE, [track('lead')]);
        mixer.gain.change('lead', -6.2, false);
        expect(calls).toEqual([
            ['gain-lead', '_smoothControl', { max: 10, min: 0, value: 4.5 }],
        ]);
        expect(mixer.store.output('lead')?.gain.value).toBe(4.5);
        expect(text()).toBe("lead: $sine('c4').track({ gain: 4.5 })\n");
    });

    test('is silence at the floor of the scale', () => {
        const { mixer, calls } = mixerOn(SOURCE, [track('lead')]);
        mixer.gain.change('lead', -60, false);
        expect(calls[0][2]).toMatchObject({ value: 0 });
    });

    test('sends nothing while the snapped gain is unchanged', () => {
        const { mixer, calls } = mixerOn(SOURCE, [track('lead')]);
        mixer.gain.change('lead', -6, false);
        mixer.gain.change('lead', -6.1, false);
        expect(calls).toHaveLength(1);
    });

    test('writes the source at the throttled rate and on release', () => {
        const { mixer, calls, text } = mixerOn(SOURCE, [track('lead')]);
        mixer.gain.change('lead', -6, false);
        mixer.gain.change('lead', -12, false);
        mixer.gain.change('lead', -18, false);
        expect(calls).toHaveLength(3);
        expect(text()).toBe("lead: $sine('c4').track({ gain: 4.5 })\n");
        vi.advanceTimersByTime(150);
        expect(text()).toBe("lead: $sine('c4').track({ gain: 3.5 })\n");

        mixer.gain.change('lead', -24, false);
        mixer.gain.commit('lead', false);
        expect(text()).toBe("lead: $sine('c4').track({ gain: 3 })\n");
        vi.advanceTimersByTime(1000);
        expect(text()).toBe("lead: $sine('c4').track({ gain: 3 })\n");
    });

    test('leaves a signal-driven gain alone', () => {
        const { mixer, calls, text } = mixerOn(SOURCE, [
            track('lead', {
                gain: { locked: true, moduleId: null, value: null },
            }),
        ]);
        mixer.gain.change('lead', -6, false);
        mixer.gain.commit('lead', false);
        mixer.gain.reset('lead');
        expect(calls).toEqual([]);
        expect(text()).toBe(SOURCE);
    });

    test('on the main strip writes $setMainGain', () => {
        const { mixer, text } = mixerOn(SOURCE, [
            track('__main__', {
                main: true,
                muteModuleId: undefined,
                sourceLocation: undefined,
            }),
        ]);
        mixer.gain.change('__main__', -6, false);
        expect(text()).toBe(`${SOURCE}$setMainGain(4.5)\n`);
    });
});

describe('a code-only fader drag', () => {
    test('writes the source and ghosts, leaving the audio', () => {
        const { mixer, calls, text } = mixerOn(SOURCE, [track('lead')]);
        mixer.gain.change('lead', -3, true);
        mixer.gain.commit('lead', true);
        expect(calls).toEqual([]);
        expect(mixer.store.output('lead')?.gain.value).toBe(5);
        expect(mixer.store.ghost('lead')).toEqual({ gain: 4.75 });
        expect(text()).toBe("lead: $sine('c4').track({ gain: 4.75 })\n");
    });

    test('drops the ghost when it lands on the audio gain', () => {
        const { mixer } = mixerOn(SOURCE, [track('lead')]);
        mixer.gain.change('lead', -3, true);
        mixer.gain.change('lead', 0, true);
        expect(mixer.store.ghost('lead')).toBeUndefined();
    });

    test('a plain drag after it brings code and audio together', () => {
        const { mixer, text } = mixerOn(SOURCE, [track('lead')]);
        mixer.gain.change('lead', -3, true);
        vi.advanceTimersByTime(1000);
        mixer.gain.change('lead', -6, false);
        expect(mixer.store.ghost('lead')).toBeUndefined();
        expect(text()).toBe("lead: $sine('c4').track({ gain: 4.5 })\n");
    });
});

describe('revert', () => {
    test('writes the audio gain back and drops the ghost', () => {
        const { mixer, calls, text } = mixerOn(SOURCE, [track('lead')]);
        mixer.gain.change('lead', -3, true);
        mixer.gain.revert('lead');
        expect(calls).toEqual([]);
        expect(text()).toBe(SOURCE);
        expect(mixer.store.ghost('lead')).toBeUndefined();
    });

    test('does nothing while code and audio agree', () => {
        const source = "lead: $sine('c4').track({ gain: 5 })\n";
        const { mixer, text } = mixerOn(source, [track('lead')]);
        mixer.gain.revert('lead');
        expect(text()).toBe(source);
    });
});

describe('reset', () => {
    test('returns a track to unity and removes its option', () => {
        const { mixer, calls, text } = mixerOn(SOURCE, [track('lead')]);
        mixer.gain.change('lead', -6, false);
        mixer.gain.reset('lead');
        expect(calls.at(-1)).toEqual([
            'gain-lead',
            '_smoothControl',
            { max: 10, min: 0, value: 5 },
        ]);
        expect(mixer.store.output('lead')?.gain.value).toBe(5);
        expect(text()).toBe(SOURCE);
    });
});
