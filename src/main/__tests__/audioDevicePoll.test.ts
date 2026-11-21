import type { DeviceReport, PatchGraph } from '@modular/core';
import { describe, expect, test, vi } from 'vitest';
import { AppliedPatchState } from '../appliedPatchState';
import {
    appliedSettingsNotice,
    pollAudioDevices,
    outputDeviceNotice,
} from '../audioDevicePoll';

const graph = { modules: [] } as unknown as PatchGraph;

const report = (over: Partial<DeviceReport> = {}): DeviceReport => ({
    engineReset: true,
    event: 'Moved',
    outputDeviceName: 'Speakers',
    outputLost: false,
    previousSampleRate: 48000,
    sampleRate: 48000,
    ...over,
});

function fakeSynth(moved: DeviceReport | null, stopped: boolean) {
    return {
        pollDevices: vi.fn(() => moved),
        isStopped: vi.fn(() => stopped),
        stop: vi.fn(),
    };
}

describe('pollAudioDevices', () => {
    test('an unchanged default leaves the engine alone', () => {
        const applied = new AppliedPatchState();
        applied.record(graph, 'song', 1);

        expect(pollAudioDevices(fakeSynth(null, false), applied)).toBeNull();
        expect(applied.baseline(1).patchGraph).not.toBeNull();
    });

    test('a move while playing stops playback', () => {
        const applied = new AppliedPatchState();
        applied.record(graph, 'song', 1);

        expect(
            pollAudioDevices(fakeSynth(report(), false), applied)
                ?.stoppedPlayback,
        ).toBe(true);
        expect(applied.baseline(1).patchGraph).toBeNull();
    });

    test('a move at the same sample rate keeps the patch playing', () => {
        const applied = new AppliedPatchState();
        applied.record(graph, 'song', 1);
        const synth = fakeSynth(report({ engineReset: false }), false);

        expect(pollAudioDevices(synth, applied)?.stoppedPlayback).toBe(false);
        expect(synth.stop).not.toHaveBeenCalled();
        expect(applied.baseline(1).patchGraph).not.toBeNull();
    });

    test('a disconnected device leaves the engine waiting for it', () => {
        const applied = new AppliedPatchState();
        applied.record(graph, 'song', 1);
        const synth = fakeSynth(
            report({ event: 'Disconnected', engineReset: false }),
            false,
        );

        expect(pollAudioDevices(synth, applied)).toMatchObject({
            engineReset: false,
            stoppedPlayback: false,
        });
        expect(synth.stop).not.toHaveBeenCalled();
        expect(applied.baseline(1).patchGraph).not.toBeNull();
    });

    test('a move while stopped stops nothing', () => {
        expect(
            pollAudioDevices(fakeSynth(report(), true), new AppliedPatchState())
                ?.stoppedPlayback,
        ).toBe(false);
    });

    test('a failed move keeps playing on a device that is still there', () => {
        const applied = new AppliedPatchState();
        applied.record(graph, 'song', 1);
        const synth = fakeSynth(
            report({ error: 'busy', engineReset: false }),
            false,
        );

        expect(pollAudioDevices(synth, applied)?.stoppedPlayback).toBe(false);
        expect(synth.stop).not.toHaveBeenCalled();
        expect(applied.baseline(1).patchGraph).not.toBeNull();
    });

    test('a failed move stops playback on a device that is gone', () => {
        const applied = new AppliedPatchState();
        applied.record(graph, 'song', 1);
        const synth = fakeSynth(
            report({ error: 'busy', engineReset: false, outputLost: true }),
            false,
        );

        expect(pollAudioDevices(synth, applied)?.stoppedPlayback).toBe(true);
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

    test('explains a buffer size the new output cannot use', () => {
        expect(
            outputDeviceNotice(
                report({ previousBufferSize: 32, bufferSize: 64 }),
                true,
            ).message,
        ).toBe(
            "Output moved to Speakers. It can't use a 32-frame buffer, so the buffer size is now 64. Playback stopped. Run the patch to start it again.",
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
                "Interface was disconnected. Shoreline goes back to it when it's reconnected.",
            persists: true,
            stopped: false,
        });
        expect(
            outputDeviceNotice(
                report({ event: 'Reconnected', engineReset: false }),
                false,
            ),
        ).toMatchObject({
            message: 'Output is back on Speakers.',
            persists: false,
        });
    });

    test('says a device changed its sample rate', () => {
        expect(
            outputDeviceNotice(
                report({ event: 'RateChanged', sampleRate: 96000 }),
                true,
            ),
        ).toMatchObject({
            message:
                'Speakers switched to 96000 Hz, so Shoreline now runs at 96000 Hz. Playback stopped. Run the patch to start it again.',
            stopped: true,
        });
        expect(
            outputDeviceNotice(
                report({ event: 'RateChanged', engineReset: false }),
                false,
            ).message,
        ).toBe('Output reopened on Speakers.');
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
    test('names a chosen device once it is connected', () => {
        expect(
            outputDeviceNotice(
                report({ event: 'Connected', outputDeviceName: 'Interface' }),
                false,
            ).message,
        ).toBe('Interface is connected, so the output is on it now.');
    });

    test('says a device that will not reopen is retried', () => {
        expect(
            outputDeviceNotice(
                report({ error: 'busy.', event: 'Reconnected' }),
                false,
            ).message,
        ).toBe("Couldn't reopen Speakers: busy. Shoreline keeps trying.");
    });

    test('names the input a stopped input reopened on', () => {
        expect(
            outputDeviceNotice(
                report({ event: 'InputReopened', inputDeviceName: 'Mic' }),
                false,
            ).message,
        ).toBe('Input reopened on Mic.');
    });

    test('says a disconnected input leaves the input off', () => {
        expect(
            outputDeviceNotice(
                report({
                    disconnectedInputDeviceName: 'Mic',
                    event: 'InputReopened',
                }),
                false,
            ).message,
        ).toBe('Mic was disconnected, so the input is off.');
    });
});

describe('appliedSettingsNotice', () => {
    test('says nothing when the patch plays on', () => {
        expect(
            appliedSettingsNotice(
                { engineReset: false },
                false,
                'Speakers',
                false,
            ),
        ).toBeNull();
    });

    test('reports what a new sample rate stopped', () => {
        expect(
            appliedSettingsNotice(
                { engineReset: true, recordingPath: '/rec/take.wav' },
                true,
                'Speakers',
                false,
            ),
        ).toEqual({
            message:
                'The audio engine restarted for the new settings. Recording stopped and saved to /rec/take.wav. Playback stopped. Run the patch to start it again.',
            persists: true,
            recordingStopped: true,
            stopped: true,
        });
    });

    test('replaces a disconnected device banner', () => {
        expect(
            appliedSettingsNotice(
                { engineReset: false },
                false,
                'Speakers',
                true,
            ),
        ).toEqual({
            message: 'Output is on Speakers.',
            persists: false,
            recordingStopped: false,
            stopped: false,
        });
    });
});
