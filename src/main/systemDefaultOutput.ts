import type { DeviceFollowReport, Synthesizer } from '@modular/core';
import type { DeviceSwitchNotice } from '../shared/ipcTypes';
import type { AppliedPatchState } from './appliedPatchState';

/** How often the system default output is checked for a change. */
export const SYSTEM_DEFAULT_POLL_MS = 1000;

type FollowingSynth = Pick<
    Synthesizer,
    'followSystemDefaultOutput' | 'isStopped' | 'stop'
>;

/**
 * Move the output to the system default when it changes (headphones
 * unplugged, say). Moving the streams rebuilds the engine empty and stopped,
 * so a patch that was playing stops; so does one left playing on a device
 * that is gone when the move fails. Returns `null` when nothing moved.
 */
export function followSystemDefaultOutput(
    synth: FollowingSynth,
    appliedPatch: AppliedPatchState,
): { report: DeviceFollowReport; stoppedPlayback: boolean } | null {
    const wasPlaying = !synth.isStopped();
    const report = synth.followSystemDefaultOutput();
    if (!report) {
        return null;
    }
    if (report.error && !report.outputLost) {
        return { report, stoppedPlayback: false };
    }
    if (report.error) {
        synth.stop();
    }
    appliedPatch.clear();
    return { report, stoppedPlayback: wasPlaying };
}

/** The banner explaining a move to a new system default output. */
export function deviceSwitchNotice(
    report: DeviceFollowReport,
    stoppedPlayback: boolean,
): DeviceSwitchNotice {
    const parts: string[] = [];
    if (report.error) {
        const device = report.outputDeviceName ?? 'the new default output';
        const reason = report.error.replace(/\.$/, '');
        parts.push(
            report.outputLost
                ? `Couldn't move to ${device}: ${reason}.`
                : `Couldn't move to ${device}: ${reason}. Audio stays on the current device.`,
        );
    } else {
        parts.push(`Output moved to ${report.outputDeviceName}.`);
        if (report.sampleRate !== report.previousSampleRate) {
            parts.push(
                report.sampleRate === report.preferredSampleRate
                    ? `The sample rate is back to ${report.sampleRate} Hz.`
                    : `It can't run at ${report.preferredSampleRate} Hz, so the sample rate is now ${report.sampleRate} Hz.`,
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
        recordingStopped: Boolean(report.recordingPath),
        stopped: stoppedPlayback,
    };
}
