import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest';

import { mixerOn, track } from './mixerHarness';

const SOURCE = "lead: $sine('c4')\nfx: $bus()\n";
const tracks = () => [
    track('lead', {
        sends: [
            { bus: 'fx', locked: false, moduleId: 'send-lead-fx', value: 0 },
        ],
    }),
    track('fx', { isBus: true }),
];

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

describe('the pan knob', () => {
    test('drives the pan and writes the option while dragging', () => {
        const { mixer, calls, text } = mixerOn(SOURCE, tracks());
        mixer.knobs.change('lead', 'pan', 2.5, false);
        expect(calls).toEqual([
            ['pan-lead', '_smoothControl', { max: 5, min: -5, value: 2.5 }],
        ]);
        expect(mixer.store.output('lead')?.pan?.value).toBe(2.5);
        expect(text()).toBe(
            "lead: $sine('c4').track({ pan: 2.5 })\nfx: $bus()\n",
        );
    });

    test('leaves the option out at its centre', () => {
        const { mixer, text } = mixerOn(SOURCE, tracks());
        mixer.knobs.commit('lead', 'pan', 2.5, false);
        mixer.knobs.commit('lead', 'pan', 0, false);
        expect(text()).toBe(SOURCE);
    });

    test('writes a drag at the throttled rate and its release at once', () => {
        const { mixer, text } = mixerOn(SOURCE, tracks());
        mixer.knobs.change('lead', 'pan', 1, false);
        mixer.knobs.change('lead', 'pan', 2, false);
        expect(text()).toContain('pan: 1 ');
        mixer.knobs.commit('lead', 'pan', 3, false);
        expect(text()).toContain('pan: 3 ');
        vi.advanceTimersByTime(1000);
        expect(text()).toContain('pan: 3 ');
    });

    test('takes no gesture while a signal drives it', () => {
        const { mixer, calls, text } = mixerOn(SOURCE, [
            track('lead', {
                pan: { locked: true, moduleId: null, value: null },
            }),
        ]);
        mixer.knobs.change('lead', 'pan', 2, false);
        mixer.knobs.commit('lead', 'pan', 2, false);
        expect(calls).toEqual([]);
        expect(text()).toBe(SOURCE);
    });
});

describe('a send dial', () => {
    test('drives the send and writes it', () => {
        const { mixer, calls, text } = mixerOn(SOURCE, tracks());
        mixer.knobs.commit('lead', 'send:fx', 1.5, false);
        expect(calls).toEqual([
            ['send-lead-fx', '_smoothControl', { max: 5, min: 0, value: 1.5 }],
        ]);
        expect(mixer.store.output('lead')?.sends?.[0].value).toBe(1.5);
        expect(text()).toBe(
            "lead: $sine('c4').track({ sends: [fx, 1.5] })\nfx: $bus()\n",
        );
    });

    test('removes the send when released at 0, not while dragging', () => {
        const { mixer, text } = mixerOn(SOURCE, tracks());
        mixer.knobs.commit('lead', 'send:fx', 1.5, false);
        vi.advanceTimersByTime(1000);
        mixer.knobs.change('lead', 'send:fx', 0, false);
        expect(text()).toContain('sends: [fx, 0]');
        mixer.knobs.commit('lead', 'send:fx', 0, false);
        expect(text()).toBe(SOURCE);
    });
});

describe('a code-only knob gesture', () => {
    test('writes the source and ghosts, leaving the audio', () => {
        const { mixer, calls, text } = mixerOn(SOURCE, tracks());
        mixer.knobs.commit('lead', 'send:fx', 2, true);
        expect(calls).toEqual([]);
        expect(mixer.store.output('lead')?.sends?.[0].value).toBe(0);
        expect(mixer.store.ghost('lead')).toEqual({ 'send:fx': 2 });
        expect(text()).toContain('sends: [fx, 2]');
    });

    test('drops the ghost when it lands on the audio value', () => {
        const { mixer, text } = mixerOn(SOURCE, tracks());
        mixer.knobs.commit('lead', 'pan', -1, true);
        mixer.knobs.commit('lead', 'pan', 0, true);
        expect(mixer.store.ghost('lead')).toBeUndefined();
        expect(text()).toBe(SOURCE);
    });

    test('needs a label to write to', () => {
        const { mixer } = mixerOn(SOURCE, [
            track('lead', { sourceLocation: undefined }),
        ]);
        mixer.knobs.commit('lead', 'pan', 2, true);
        expect(mixer.store.ghost('lead')).toBeUndefined();
    });
});
