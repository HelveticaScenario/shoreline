import type { CurrentAudioState } from '../../../shared/ipcTypes';

/**
 * The panel's pending choices. An output of `null` follows the system default
 * output; an input of `null` is no input.
 */
export interface AudioSelection {
    hostId: string | null;
    outputDeviceId: string | null;
    inputDeviceId: string | null;
    sampleRate: number | null;
    bufferSize: number | null;
}

export const EMPTY_SELECTION: AudioSelection = {
    bufferSize: null,
    hostId: null,
    inputDeviceId: null,
    outputDeviceId: null,
    sampleRate: null,
};

/**
 * The sample rate and buffer size the panel shows for `state`: the chosen ones
 * while the chosen output is not connected, since the default output plays at
 * what it allows of them until it is; otherwise the ones the streams run at.
 */
export function shownSettings(state: CurrentAudioState): {
    sampleRate: number;
    bufferSize: number | null;
} {
    const waiting =
        state.chosenOutputDeviceId != null &&
        state.chosenOutputDeviceId !== state.outputDeviceId;
    return waiting
        ? {
              bufferSize: state.chosenBufferSize ?? null,
              sampleRate: state.chosenSampleRate,
          }
        : {
              bufferSize: state.bufferSize ?? null,
              sampleRate: state.sampleRate,
          };
}

/** The selection that shows `state` with nothing pending. */
export function shownSelection(state: CurrentAudioState): AudioSelection {
    return {
        hostId: state.hostId,
        inputDeviceId: state.inputDeviceId ?? null,
        outputDeviceId: state.chosenOutputDeviceId ?? null,
        ...shownSettings(state),
    };
}

/** The selection after applying, at the settings the streams opened with. */
export function appliedSelection(state: CurrentAudioState): AudioSelection {
    return {
        ...shownSelection(state),
        bufferSize: state.bufferSize ?? null,
        sampleRate: state.sampleRate,
    };
}

/** `selection` with `patch` over it; `selection` itself when nothing differs. */
export function patchSelection(
    selection: AudioSelection,
    patch: Partial<AudioSelection>,
): AudioSelection {
    const next = { ...selection, ...patch };
    return sameSelection(selection, next) ? selection : next;
}

export function sameSelection(a: AudioSelection, b: AudioSelection): boolean {
    return (
        a.hostId === b.hostId &&
        a.outputDeviceId === b.outputDeviceId &&
        a.inputDeviceId === b.inputDeviceId &&
        a.sampleRate === b.sampleRate &&
        a.bufferSize === b.bufferSize
    );
}

/**
 * `selected` with each choice still at its `was` value moved to its `now`
 * value; a choice that differs from `was` stays.
 */
export function followSelection(
    selected: AudioSelection,
    was: AudioSelection,
    now: AudioSelection,
): AudioSelection {
    const follow = <T>(choice: T, previous: T, current: T) =>
        choice === previous ? current : choice;
    return patchSelection(selected, {
        bufferSize: follow(selected.bufferSize, was.bufferSize, now.bufferSize),
        hostId: follow(selected.hostId, was.hostId, now.hostId),
        inputDeviceId: follow(
            selected.inputDeviceId,
            was.inputDeviceId,
            now.inputDeviceId,
        ),
        outputDeviceId: follow(
            selected.outputDeviceId,
            was.outputDeviceId,
            now.outputDeviceId,
        ),
        sampleRate: follow(selected.sampleRate, was.sampleRate, now.sampleRate),
    });
}
