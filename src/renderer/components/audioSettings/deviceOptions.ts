import type {
    AudioDeviceInfo,
    CurrentAudioState,
    DeviceCacheSnapshot,
} from '../../../shared/ipcTypes';
import { shownSettings, type AudioSelection } from './audioSelection';

/** The settings the engine opens with when none are chosen, where the devices allow them. */
export const PREFERRED_SAMPLE_RATE = 48_000;
const PREFERRED_BUFFER_SIZE = 256;

const POWER_OF_2_BUFFER_SIZES = [64, 128, 256, 512, 1024, 2048, 4096];

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
