import type { DeviceFollowReport, Synthesizer } from '@modular/core';
import type { OutputDeviceNotice } from '../shared/ipcTypes';
import type { AppliedPatchState } from './appliedPatchState';

/** How often the output device is checked. */
export const OUTPUT_DEVICE_POLL_MS = 1000;

type FollowingSynth = Pick<
    Synthesizer,
    'followOutputDevice' | 'isStopped' | 'stop'
>;

/**
 * Keep the output on a device that can play: follow the system default when
 * it changes (headphones unplugged, say), and reopen an output whose device
 * went away once it is back. The patch keeps playing, or waits for a device
 * that went away, when the device runs at the engine's sample rate.
 * Otherwise the engine is rebuilt empty and stopped, so a patch that was
 * playing stops; so does one left playing on a device that is gone when a
 * move fails. Returns `null` when nothing changed.
 */
export function followOutputDevice(
    synth: FollowingSynth,
    appliedPatch: AppliedPatchState,
): {
    report: DeviceFollowReport;
    stoppedPlayback: boolean;
    /** The engine no longer holds what was sent to it. */
    engineReset: boolean;
} | null {
    const wasPlaying = !synth.isStopped();
    const report = synth.followOutputDevice();
    if (!report) {
        return null;
    }
    if (
        report.engineKept ||
        report.event === 'Disconnected' ||
        (report.error && !report.outputLost)
    ) {
        return { report, stoppedPlayback: false, engineReset: false };
    }
    if (report.error) {
        synth.stop();
    }
    appliedPatch.clear();
    return { report, stoppedPlayback: wasPlaying, engineReset: true };
}

/** The banner explaining what happened to the output device. */
export function outputDeviceNotice(
    report: DeviceFollowReport,
    stoppedPlayback: boolean,
): OutputDeviceNotice {
    const parts: string[] = [];
    if (report.event === 'Disconnected') {
        parts.push(
            `${report.outputDeviceName ?? 'The output device'} was disconnected. Playback picks up where it left off when it's back.`,
        );
    } else if (report.error) {
        const device = report.outputDeviceName ?? 'the new default output';
        const reason = report.error.replace(/\.$/, '');
        parts.push(
            report.outputLost
                ? `Couldn't move to ${device}: ${reason}.`
                : `Couldn't move to ${device}: ${reason}. Audio stays on the current device.`,
        );
    } else {
        parts.push(
            report.event === 'Reconnected'
                ? `Output is back on ${report.outputDeviceName}.`
                : `Output moved to ${report.outputDeviceName}.`,
        );
        if (report.sampleRate !== report.previousSampleRate) {
            parts.push(
                `It can't run at ${report.previousSampleRate} Hz, so the sample rate is now ${report.sampleRate} Hz.`,
            );
        }
        if (report.unopenedInputDeviceName) {
            parts.push(
                `Input is off: ${report.unopenedInputDeviceName} can't run at ${report.sampleRate} Hz.`,
            );
        }
    }
    if (report.recordingPath) {
        parts.push(`Recording stopped and saved to ${report.recordingPath}.`);
    }
    if (stoppedPlayback) {
        parts.push('Playback stopped. Run the patch to start it again.');
    }
    return {
        message: parts.join(' '),
        persists: stoppedPlayback || report.event === 'Disconnected',
        recordingStopped: Boolean(report.recordingPath),
        stopped: stoppedPlayback,
    };
}
