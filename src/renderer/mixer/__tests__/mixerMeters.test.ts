import type { VuMeterFrame } from '@modular/core';
import { beforeEach, describe, expect, test, vi } from 'vitest';

import { drawVuMeter } from '../../app/vuMeter';
import { mixerOn, track } from './mixerHarness';

vi.mock('../../app/vuMeterColors', () => ({ readVuMeterColors: () => ({}) }));
vi.mock('../../app/vuMeter', async (importOriginal) => ({
    ...(await importOriginal<typeof import('../../app/vuMeter')>()),
    drawVuMeter: vi.fn(),
}));

const drawn = vi.mocked(drawVuMeter);
const canvas = () => ({}) as HTMLCanvasElement;
const frame = (moduleId: string, over: Partial<VuMeterFrame> = {}) =>
    ({ moduleId, peak: [1, 1], rms: [0.5, 0.5], ...over }) as VuMeterFrame;

beforeEach(() => drawn.mockClear());

describe('meter canvases', () => {
    test('draw the frames of the module they were registered with', () => {
        const { mixer } = mixerOn('', [track('lead'), track('bass')]);
        const lead = canvas();
        const bass = canvas();
        mixer.meters.registerCanvas('lead', lead, 'tap-lead', 2);
        mixer.meters.registerCanvas('bass', bass, 'tap-bass', 1);
        mixer.meters.drawFrames([frame('tap-bass'), frame('tap-other')]);
        expect(drawn).toHaveBeenCalledTimes(1);
        expect(drawn.mock.calls[0][0]).toBe(bass);
        expect(drawn.mock.calls[0][1]).toHaveLength(1);
    });

    test('redraw as silence across their channels before any frame', () => {
        const { mixer } = mixerOn('', [track('lead')]);
        mixer.meters.registerCanvas('lead', canvas(), 'tap-lead', 2);
        mixer.meters.redraw('lead');
        expect(drawn.mock.calls[0][1]).toEqual([
            { fastDb: -Infinity, peakDb: -Infinity, rmsDb: -Infinity },
            { fastDb: -Infinity, peakDb: -Infinity, rmsDb: -Infinity },
        ]);
    });

    test('fall to silence when the engine stops', () => {
        const { mixer } = mixerOn('', [track('lead')]);
        mixer.meters.registerCanvas('lead', canvas(), 'tap-lead', 2);
        mixer.meters.drawFrames([frame('tap-lead')]);
        mixer.meters.silence();
        expect(drawn.mock.calls.at(-1)?.[1][0].rmsDb).toBe(-Infinity);
    });

    test('stop drawing once unregistered', () => {
        const { mixer } = mixerOn('', [track('lead')]);
        mixer.meters.registerCanvas('lead', canvas(), 'tap-lead', 2);
        mixer.meters.unregisterCanvas('lead');
        mixer.meters.drawFrames([frame('tap-lead')]);
        expect(drawn).not.toHaveBeenCalled();
    });
});

describe('locked knobs', () => {
    test('follow their control in the frame across their range', () => {
        const { mixer } = mixerOn('', [
            track('lead', {
                pan: {
                    locked: true,
                    moduleId: null,
                    sourceIndex: 1,
                    value: null,
                },
            }),
        ]);
        const pointer = { setAttribute: vi.fn() };
        const svg = {
            querySelector: (selector: string) =>
                selector === '.vu-knob-pointer' ? pointer : null,
        } as unknown as SVGSVGElement;
        mixer.meters.registerCanvas('lead', canvas(), 'tap-lead', 2);
        mixer.meters.registerKnob('lead', 'pan', svg, {
            max: 5,
            min: -5,
            origin: 0,
        });
        mixer.meters.drawFrames([frame('tap-lead', { controls: [0, 5] })]);
        expect(pointer.setAttribute).toHaveBeenCalledWith(
            'transform',
            'rotate(135 11 11)',
        );

        pointer.setAttribute.mockClear();
        mixer.meters.unregisterKnob('lead', 'pan');
        mixer.meters.drawFrames([frame('tap-lead', { controls: [0, 5] })]);
        expect(pointer.setAttribute).not.toHaveBeenCalled();
    });
});
