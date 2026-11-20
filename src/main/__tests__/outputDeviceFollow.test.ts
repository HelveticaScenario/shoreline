import type { DeviceFollowReport, PatchGraph } from '@modular/core';
import { describe, expect, test, vi } from 'vitest';
import { AppliedPatchState } from '../appliedPatchState';
import { outputDeviceNotice, followOutputDevice } from '../outputDeviceFollow';

const graph = { modules: [] } as unknown as PatchGraph;

const report = (
    over: Partial<DeviceFollowReport> = {},
): DeviceFollowReport => ({
    engineKept: false,
    event: 'Moved',
    outputDeviceName: 'Speakers',
    outputLost: false,
    previousSampleRate: 48000,
    sampleRate: 48000,
    ...over,
});

function fakeSynth(moved: DeviceFollowReport | null, stopped: boolean) {
    return {
        followOutputDevice: vi.fn(() => moved),
        isStopped: vi.fn(() => stopped),
        stop: vi.fn(),
    };
}

describe('followOutputDevice', () => {
    test('an unchanged default leaves the engine alone', () => {
        const applied = new AppliedPatchState();
        applied.record(graph, 'song', 1);

        expect(followOutputDevice(fakeSynth(null, false), applied)).toBeNull();
        expect(applied.baseline(1).patchGraph).not.toBeNull();
    });

    test('a move while playing stops playback', () => {
        const applied = new AppliedPatchState();
        applied.record(graph, 'song', 1);

        expect(
            followOutputDevice(fakeSynth(report(), false), applied)
                ?.stoppedPlayback,
        ).toBe(true);
        expect(applied.baseline(1).patchGraph).toBeNull();
    });

    test('a move at the same sample rate keeps the patch playing', () => {
        const applied = new AppliedPatchState();
        applied.record(graph, 'song', 1);
        const synth = fakeSynth(report({ engineKept: true }), false);

        expect(followOutputDevice(synth, applied)?.stoppedPlayback).toBe(false);
        expect(synth.stop).not.toHaveBeenCalled();
        expect(applied.baseline(1).patchGraph).not.toBeNull();
    });

    test('a disconnected device leaves the engine waiting for it', () => {
        const applied = new AppliedPatchState();
        applied.record(graph, 'song', 1);
        const synth = fakeSynth(report({ event: 'Disconnected' }), false);

        expect(followOutputDevice(synth, applied)).toMatchObject({
            engineReset: false,
            stoppedPlayback: false,
        });
        expect(synth.stop).not.toHaveBeenCalled();
        expect(applied.baseline(1).patchGraph).not.toBeNull();
    });

    test('a move while stopped stops nothing', () => {
        expect(
            followOutputDevice(
                fakeSynth(report(), true),
                new AppliedPatchState(),
            )?.stoppedPlayback,
        ).toBe(false);
    });

    test('a failed move keeps playing on a device that is still there', () => {
        const applied = new AppliedPatchState();
        applied.record(graph, 'song', 1);
        const synth = fakeSynth(report({ error: 'busy' }), false);

        expect(followOutputDevice(synth, applied)?.stoppedPlayback).toBe(false);
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

        expect(followOutputDevice(synth, applied)?.stoppedPlayback).toBe(true);
        expect(synth.stop).toHaveBeenCalled();
        expect(applied.baseline(1).patchGraph).toBeNull();
    });
});

describe('outputDeviceNotice', () => {
    test('names the new device', () => {
        expect(outputDeviceNotice(report(), false)).toEqual({
            message: 'Output moved to Speakers.',
            persists: false,
            recordingStopped: false,
            stopped: false,
        });
    });

    test('explains a sample rate the new output cannot run at', () => {
        expect(
            outputDeviceNotice(report({ previousSampleRate: 192000 }), false)
                .message,
        ).toBe(
            "Output moved to Speakers. It can't run at 192000 Hz, so the sample rate is now 48000 Hz.",
        );
    });

    test('explains an input left off', () => {
        expect(
            outputDeviceNotice(
                report({ unopenedInputDeviceName: 'Interface' }),
                false,
            ).message,
        ).toBe(
            "Output moved to Speakers. Input is off: Interface can't run at 48000 Hz.",
        );
    });

    test('reports a recording the move ended', () => {
        expect(
            outputDeviceNotice(
                report({ recordingPath: '/rec/take.wav' }),
                true,
            ),
        ).toEqual({
            message:
                'Output moved to Speakers. Recording stopped and saved to /rec/take.wav. Playback stopped. Run the patch to start it again.',
            persists: true,
            recordingStopped: true,
            stopped: true,
        });
    });

    test('says a device went away and came back', () => {
        expect(
            outputDeviceNotice(
                report({
                    event: 'Disconnected',
                    outputDeviceName: 'Interface',
                }),
                false,
            ),
        ).toMatchObject({
            message:
                "Interface was disconnected. Playback picks up where it left off when it's back.",
            persists: true,
            stopped: false,
        });
        expect(
            outputDeviceNotice(
                report({ event: 'Reconnected', engineKept: true }),
                false,
            ),
        ).toMatchObject({
            message: 'Output is back on Speakers.',
            persists: false,
        });
    });

    test('explains a failed move', () => {
        expect(
            outputDeviceNotice(
                report({ error: 'busy', outputDeviceName: undefined }),
                false,
            ).message,
        ).toBe(
            "Couldn't move to the new default output: busy. Audio stays on the current device.",
        );
        expect(
            outputDeviceNotice(
                report({ error: 'busy', outputLost: true }),
                true,
            ).message,
        ).toBe(
            "Couldn't move to Speakers: busy. Playback stopped. Run the patch to start it again.",
        );
    });
});
