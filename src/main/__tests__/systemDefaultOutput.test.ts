import type { DeviceFollowReport, PatchGraph } from '@modular/core';
import { describe, expect, test, vi } from 'vitest';
import { AppliedPatchState } from '../appliedPatchState';
import {
    deviceSwitchNotice,
    followSystemDefaultOutput,
} from '../systemDefaultOutput';

const graph = { modules: [] } as unknown as PatchGraph;

const report = (
    over: Partial<DeviceFollowReport> = {},
): DeviceFollowReport => ({
    outputDeviceName: 'Speakers',
    outputLost: false,
    preferredSampleRate: 48000,
    previousSampleRate: 48000,
    sampleRate: 48000,
    ...over,
});

function fakeSynth(moved: DeviceFollowReport | null, stopped: boolean) {
    return {
        followSystemDefaultOutput: vi.fn(() => moved),
        isStopped: vi.fn(() => stopped),
        stop: vi.fn(),
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

    test('a failed move keeps playing on a device that is still there', () => {
        const applied = new AppliedPatchState();
        applied.record(graph, 'song', 1);
        const synth = fakeSynth(report({ error: 'busy' }), false);

        expect(followSystemDefaultOutput(synth, applied)?.stoppedPlayback).toBe(
            false,
        );
        expect(synth.stop).not.toHaveBeenCalled();
        expect(applied.baseline(1).patchGraph).not.toBeNull();
    });

    test('a failed move stops playback on a device that is gone', () => {
        const applied = new AppliedPatchState();
        applied.record(graph, 'song', 1);
        const synth = fakeSynth(
            report({ error: 'busy', outputLost: true }),
            false,
        );

        expect(followSystemDefaultOutput(synth, applied)?.stoppedPlayback).toBe(
            true,
        );
        expect(synth.stop).toHaveBeenCalled();
        expect(applied.baseline(1).patchGraph).toBeNull();
    });
});

describe('deviceSwitchNotice', () => {
    test('names the new device', () => {
        expect(deviceSwitchNotice(report(), false)).toEqual({
            message: 'Output moved to Speakers.',
            recordingStopped: false,
            stopped: false,
        });
    });

    test('explains a sample rate the new output cannot run at', () => {
        expect(
            deviceSwitchNotice(
                report({
                    preferredSampleRate: 192000,
                    previousSampleRate: 192000,
                }),
                false,
            ).message,
        ).toBe(
            "Output moved to Speakers. It can't run at 192000 Hz, so the sample rate is now 48000 Hz.",
        );
    });

    test('says when the sample rate returns to the one asked for', () => {
        expect(
            deviceSwitchNotice(
                report({
                    preferredSampleRate: 96000,
                    previousSampleRate: 48000,
                    sampleRate: 96000,
                }),
                false,
            ).message,
        ).toBe(
            'Output moved to Speakers. The sample rate is back to 96000 Hz.',
        );
    });

    test('explains an input left off', () => {
        expect(
            deviceSwitchNotice(
                report({ unopenedInputDeviceName: 'Interface' }),
                false,
            ).message,
        ).toBe(
            "Output moved to Speakers. Input is off: Interface can't run at 48000 Hz.",
        );
    });

    test('reports a recording the move ended', () => {
        expect(
            deviceSwitchNotice(
                report({ recordingPath: '/rec/take.wav' }),
                true,
            ),
        ).toEqual({
            message:
                'Output moved to Speakers. Recording stopped and saved to /rec/take.wav. Playback stopped. Run the patch to start it again.',
            recordingStopped: true,
            stopped: true,
        });
    });

    test('explains a failed move', () => {
        expect(
            deviceSwitchNotice(
                report({ error: 'busy', outputDeviceName: undefined }),
                false,
            ).message,
        ).toBe(
            "Couldn't move to the new default output: busy. Audio stays on the current device.",
        );
        expect(
            deviceSwitchNotice(
                report({ error: 'busy', outputLost: true }),
                true,
            ).message,
        ).toBe(
            "Couldn't move to Speakers: busy. Playback stopped. Run the patch to start it again.",
        );
    });
});
