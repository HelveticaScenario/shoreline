import type { DeviceFollowReport, Synthesizer } from '@modular/core';
import type { DeviceSwitchNotice } from '../shared/ipcTypes';
import type { AppliedPatchState } from './appliedPatchState';

/** How often the system default output is checked for a change. */
export const SYSTEM_DEFAULT_POLL_MS = 1000;

type FollowingSynth = Pick<
    Synthesizer,
    'followSystemDefaultOutput' | 'isStopped'
>;

/**
 * Move the output to the system default when it changes (headphones
 * unplugged, say). Moving the streams rebuilds the engine empty and stopped,
 * so a patch that was playing stops. Returns `null` when nothing moved.
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
    if (report.error) {
        return { report, stoppedPlayback: false };
    }
    appliedPatch.clear();
    return { report, stoppedPlayback: wasPlaying };
}

/** The banner explaining a move to a new system default output. */
export function deviceSwitchNotice(
    report: DeviceFollowReport,
    stoppedPlayback: boolean,
): DeviceSwitchNotice {
    if (report.error) {
        return {
            message: `Couldn't move to the new system default device, so audio stays where it was: ${report.error}`,
            stopped: false,
        };
    }
    const parts = [`Output moved to ${report.outputDeviceName}.`];
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
    if (stoppedPlayback) {
        parts.push('Playback stopped. Run the patch to start it again.');
    }
    return { message: parts.join(' '), stopped: stoppedPlayback };
}
