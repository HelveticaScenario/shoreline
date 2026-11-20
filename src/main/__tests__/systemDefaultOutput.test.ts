import type { DeviceFollowReport } from '@modular/core';
import { describe, expect, test, vi } from 'vitest';
import { AppliedPatchState } from '../appliedPatchState';
import {
    deviceSwitchNotice,
    followSystemDefaultOutput,
} from '../systemDefaultOutput';
import type { PatchGraph } from '@modular/core';

const graph = { modules: [] } as unknown as PatchGraph;

const report = (
    over: Partial<DeviceFollowReport> = {},
): DeviceFollowReport => ({
    outputDeviceName: 'Speakers',
    previousSampleRate: 48000,
    sampleRate: 48000,
    ...over,
});

function fakeSynth(moved: DeviceFollowReport | null, stopped: boolean) {
    return {
        followSystemDefaultOutput: vi.fn(() => moved),
        isStopped: vi.fn(() => stopped),
    };
}

describe('followSystemDefaultOutput', () => {
    test('an unchanged default leaves the engine alone', () => {
        const applied = new AppliedPatchState();
        applied.record(graph, 'song', 1);

        expect(
            followSystemDefaultOutput(fakeSynth(null, false), applied),
        ).toBeNull();
        expect(applied.baseline(1).patchGraph).not.toBeNull();
    });

    test('a move while playing stops playback', () => {
        const applied = new AppliedPatchState();
        applied.record(graph, 'song', 1);

        expect(
            followSystemDefaultOutput(fakeSynth(report(), false), applied)
                ?.stoppedPlayback,
        ).toBe(true);
        expect(applied.baseline(1).patchGraph).toBeNull();
    });

    test('a move while stopped stops nothing', () => {
        expect(
            followSystemDefaultOutput(
                fakeSynth(report(), true),
                new AppliedPatchState(),
            )?.stoppedPlayback,
        ).toBe(false);
    });

    test('streams that failed to open keep the playing patch', () => {
        const applied = new AppliedPatchState();
        applied.record(graph, 'song', 1);

        expect(
            followSystemDefaultOutput(
                fakeSynth(report({ error: 'busy' }), false),
                applied,
            )?.stoppedPlayback,
        ).toBe(false);
        expect(applied.baseline(1).patchGraph).not.toBeNull();
    });
});

describe('deviceSwitchNotice', () => {
    test('names the new device', () => {
        expect(deviceSwitchNotice(report(), false)).toEqual({
            message: 'Output moved to Speakers.',
            stopped: false,
        });
    });

    test('explains a sample rate the new output cannot run at', () => {
        expect(
            deviceSwitchNotice(
                report({ previousSampleRate: 192000, sampleRate: 48000 }),
                false,
            ).message,
        ).toBe(
            "Output moved to Speakers. It can't run at 192000 Hz, so the sample rate is now 48000 Hz.",
        );
    });

    test('explains an input left off', () => {
        expect(
            deviceSwitchNotice(
                report({
                    previousSampleRate: 192000,
                    sampleRate: 48000,
                    unopenedInputDeviceName: 'Interface',
                }),
                false,
            ).message,
        ).toBe(
            "Output moved to Speakers. It can't run at 192000 Hz, so the sample rate is now 48000 Hz. Input is off: Interface can't run at 48000 Hz.",
        );
    });

    test('explains a stop', () => {
        expect(deviceSwitchNotice(report(), true)).toEqual({
            message:
                'Output moved to Speakers. Playback stopped. Run the patch to start it again.',
            stopped: true,
        });
    });
});
