import type {
    AudioDeviceInfo,
    CurrentAudioState,
    DeviceCacheSnapshot,
} from '../../../shared/ipcTypes';

/** The settings the engine opens with when none are chosen, where the devices allow them. */
const PREFERRED_SAMPLE_RATE = 48_000;
const PREFERRED_BUFFER_SIZE = 256;

const POWER_OF_2_BUFFER_SIZES = [64, 128, 256, 512, 1024, 2048, 4096];

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

/** What a selection offers: the host's devices and the settings they can run. */
export interface DeviceOptions {
    outputDevices: AudioDeviceInfo[];
    inputDevices: AudioDeviceInfo[];
    /** The device a selection of the system default output plays on. */
    followedOutputDevice: AudioDeviceInfo | null;
    selectedOutputDevice: AudioDeviceInfo | null;
    selectedInputDevice: AudioDeviceInfo | null;
    /** The selected output when the host does not list it. */
    unlistedOutputDeviceId: string | null;
    /** The output whose own sample rate is the fallback choice. */
    capabilityOutputDevice: AudioDeviceInfo | null;
    sampleRates: number[];
    bufferSizes: number[];
}

/**
 * The sample rate and buffer size the panel shows for `state`: the chosen ones
 * while the chosen output is not connected, since the default output plays at
 * what it allows of them until it is; otherwise the ones the streams run at.
 */
function shownSettings(state: CurrentAudioState): {
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

/**
 * Compute intersection of sample rates supported by given devices.
 * If only output device is provided, returns its supported rates.
 */
function computeCommonSampleRates(
    outputDevice: AudioDeviceInfo | null,
    inputDevice: AudioDeviceInfo | null,
): number[] {
    if (!outputDevice) {
        return [];
    }

    const outputRates = outputDevice.supportedSampleRates || [];
    if (!inputDevice) {
        return [...outputRates].sort((a, b) => a - b);
    }

    const inputRates = new Set(inputDevice.supportedSampleRates || []);
    return outputRates.filter((r) => inputRates.has(r)).sort((a, b) => a - b);
}

/** `options` with `chosen` among them, in order. */
function withChosen(options: number[], chosen: number | null): number[] {
    if (chosen === null || options.includes(chosen)) {
        return options;
    }
    return [...options, chosen].sort((a, b) => a - b);
}

/** The power-of-2 buffer sizes within the range both devices support. */
function computeBufferSizes(
    outputDevice: AudioDeviceInfo | null,
    inputDevice: AudioDeviceInfo | null,
): number[] {
    if (!outputDevice) {
        return [];
    }

    let minSize = outputDevice.bufferSizeRange?.min ?? 64;
    let maxSize = outputDevice.bufferSizeRange?.max ?? 4096;

    if (inputDevice?.bufferSizeRange) {
        minSize = Math.max(minSize, inputDevice.bufferSizeRange.min);
        maxSize = Math.min(maxSize, inputDevice.bufferSizeRange.max);
    }

    return POWER_OF_2_BUFFER_SIZES.filter(
        (size) => size >= minSize && size <= maxSize,
    );
}

export function deviceOptions(
    deviceCache: DeviceCacheSnapshot | null,
    engine: CurrentAudioState | null,
    selection: AudioSelection,
): DeviceOptions {
    const host = selection.hostId
        ? deviceCache?.hosts.find((h) => h.hostId === selection.hostId)
        : undefined;
    const outputDevices = host?.outputDevices ?? [];
    const inputDevices = host?.inputDevices ?? [];

    const { outputDeviceId, inputDeviceId } = selection;
    const followedOutputDevice = outputDevices.find((d) => d.isDefault) ?? null;
    const selectedOutputDevice =
        outputDeviceId === null
            ? followedOutputDevice
            : (outputDevices.find((d) => d.id === outputDeviceId) ?? null);
    const selectedInputDevice =
        inputDevices.find((d) => d.id === inputDeviceId) ?? null;
    const unlistedOutputDeviceId =
        outputDeviceId !== null &&
        !outputDevices.some((d) => d.id === outputDeviceId)
            ? outputDeviceId
            : null;

    // A chosen output that is not connected offers what the device playing in
    // its place allows, along with the settings already chosen for it.
    const capabilityOutputDevice = unlistedOutputDeviceId
        ? (outputDevices.find((d) => d.id === engine?.outputDeviceId) ?? null)
        : selectedOutputDevice;
    const chosenForUnlisted =
        unlistedOutputDeviceId && engine ? shownSettings(engine) : null;

    return {
        bufferSizes: withChosen(
            computeBufferSizes(capabilityOutputDevice, selectedInputDevice),
            chosenForUnlisted?.bufferSize ?? null,
        ),
        capabilityOutputDevice,
        followedOutputDevice,
        inputDevices,
        outputDevices,
        sampleRates: withChosen(
            computeCommonSampleRates(
                capabilityOutputDevice,
                selectedInputDevice,
            ),
            chosenForUnlisted?.sampleRate ?? null,
        ),
        selectedInputDevice,
        selectedOutputDevice,
        unlistedOutputDeviceId,
    };
}

/**
 * The sample rate and buffer size `selection` holds that `options` cannot
 * run, replaced by the engine's defaults where the devices allow them, then
 * by the output's own rate and the lowest buffer size.
 */
export function runnableSettings(
    selection: AudioSelection,
    options: DeviceOptions,
): Partial<AudioSelection> {
    const { sampleRates, bufferSizes } = options;
    const patch: Partial<AudioSelection> = {};

    if (
        selection.sampleRate === null ||
        !sampleRates.includes(selection.sampleRate)
    ) {
        const deviceDefault =
            options.capabilityOutputDevice?.sampleRate ?? null;
        patch.sampleRate = sampleRates.includes(PREFERRED_SAMPLE_RATE)
            ? PREFERRED_SAMPLE_RATE
            : deviceDefault !== null && sampleRates.includes(deviceDefault)
              ? deviceDefault
              : sampleRates.length > 0
                ? sampleRates[sampleRates.length - 1]
                : null;
    }

    if (
        selection.bufferSize === null ||
        !bufferSizes.includes(selection.bufferSize)
    ) {
        patch.bufferSize = bufferSizes.includes(PREFERRED_BUFFER_SIZE)
            ? PREFERRED_BUFFER_SIZE
            : bufferSizes.length > 0
              ? Math.min(...bufferSizes)
              : null;
    }

    return patch;
}
