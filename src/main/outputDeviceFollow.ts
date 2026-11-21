import type {
    DeviceFollowReport,
    StreamsChange,
    Synthesizer,
} from '@modular/core';
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
 * went away or changed its sample rate. The patch keeps playing, or waits for
 * a device that went away, when the device runs at the engine's sample rate
 * and buffer size.
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
    if (!report.engineReset && !report.outputLost) {
        return { report, stoppedPlayback: false, engineReset: false };
    }
    if (report.outputLost) {
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
    const name = report.outputDeviceName ?? 'The output device';
    if (report.event === 'Disconnected') {
        parts.push(
            `${name} was disconnected. Shoreline goes back to it when it's reconnected.`,
        );
    } else if (report.event === 'InputReopened') {
        parts.push(
            report.error
                ? `The input stopped and couldn't reopen: ${report.error.replace(/\.$/, '')}.`
                : report.disconnectedInputDeviceName
                  ? `${report.disconnectedInputDeviceName} was disconnected, so the input is off.`
                  : report.inputDeviceName
                    ? `Input reopened on ${report.inputDeviceName}.`
                    : 'The input stopped, so the input is off.',
        );
    } else if (report.event === 'RateChanged' && !report.error) {
        parts.push(
            report.sampleRate === report.previousSampleRate
                ? `Output reopened on ${name}.`
                : `${name} switched to ${report.sampleRate} Hz, so Shoreline now runs at ${report.sampleRate} Hz.`,
        );
    } else if (report.error) {
        const device = report.outputDeviceName ?? 'the new default output';
        const reason = report.error.replace(/\.$/, '');
        parts.push(
            report.event === 'Reconnected' || report.event === 'RateChanged'
                ? `Couldn't reopen ${name}: ${reason}. Shoreline keeps trying.`
                : report.outputLost
                  ? `Couldn't move to ${device}: ${reason}.`
                  : `Couldn't move to ${device}: ${reason}. Audio stays on the current device.`,
        );
    } else {
        parts.push(
            report.event === 'Reconnected'
                ? `Output is back on ${report.outputDeviceName}.`
                : report.event === 'Connected'
                  ? `${report.outputDeviceName} is connected, so the output is on it now.`
                  : `Output moved to ${report.outputDeviceName}.`,
        );
        if (report.sampleRate !== report.previousSampleRate) {
            parts.push(
                `It can't run at ${report.previousSampleRate} Hz, so the sample rate is now ${report.sampleRate} Hz.`,
            );
        }
    }
    const opened = !report.error && report.event !== 'Disconnected';
    if (opened && report.bufferSize !== report.previousBufferSize) {
        parts.push(
            `It can't use a ${report.previousBufferSize ?? 'default'}-frame buffer, so the buffer size is now ${report.bufferSize ?? "the system's default"}.`,
        );
    }
    if (opened && report.unopenedInputDeviceName) {
        parts.push(
            `Input is off: ${report.unopenedInputDeviceName} can't run at ${report.sampleRate} Hz.`,
        );
    }
    return withEngineReset(
        parts,
        report.recordingPath,
        stoppedPlayback,
        report.event === 'Disconnected',
    );
}

/**
 * The banner after the user applies new audio settings: what a new sample
 * rate or buffer size stopped, or where the output is now in place of a
 * disconnected device's banner. `null` when there is nothing to say.
 */
export function appliedSettingsNotice(
    change: StreamsChange,
    stoppedPlayback: boolean,
    outputDeviceName: string | null | undefined,
    replacesDisconnected: boolean,
): OutputDeviceNotice | null {
    const parts: string[] = [];
    if (replacesDisconnected && outputDeviceName) {
        parts.push(`Output is on ${outputDeviceName}.`);
    }
    if (change.engineReset && (stoppedPlayback || change.recordingPath)) {
        parts.push('The audio engine restarted for the new settings.');
    }
    if (!parts.length) {
        return null;
    }
    return withEngineReset(parts, change.recordingPath, stoppedPlayback, false);
}

function withEngineReset(
    parts: string[],
    recordingPath: string | null | undefined,
    stoppedPlayback: boolean,
    persists: boolean,
): OutputDeviceNotice {
    if (recordingPath) {
        parts.push(`Recording stopped and saved to ${recordingPath}.`);
    }
    if (stoppedPlayback) {
        parts.push('Playback stopped. Run the patch to start it again.');
    }
    return {
        message: parts.join(' '),
        persists: stoppedPlayback || persists,
        recordingStopped: Boolean(recordingPath),
        stopped: stoppedPlayback,
    };
}
