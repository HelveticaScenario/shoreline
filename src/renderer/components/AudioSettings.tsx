import React, {
    useCallback,
    useEffect,
    useMemo,
    useRef,
    useState,
} from 'react';
import electronAPI from '../electronAPI';
import type {
    AudioDeviceInfo,
    CurrentAudioState,
    DeviceCacheSnapshot,
} from '../../shared/ipcTypes';

const DEVICE_REFRESH_INTERVAL_MS = 2000;

interface AudioSettingsTabProps {
    isActive: boolean;
}

/** The settings the engine opens with when none are chosen, where the devices allow them. */
const PREFERRED_SAMPLE_RATE = 48_000;
const PREFERRED_BUFFER_SIZE = 256;

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

/**
 * Compute buffer sizes available for given devices.
 * Returns power-of-2 values (64, 128, 256, 512, 1024, 2048) within the supported range.
 */
function computeBufferSizes(
    outputDevice: AudioDeviceInfo | null,
    inputDevice: AudioDeviceInfo | null,
): number[] {
    const powerOf2Sizes = [64, 128, 256, 512, 1024, 2048, 4096];

    if (!outputDevice) {
        return [];
    }

    // Find the common range
    let minSize = outputDevice.bufferSizeRange?.min ?? 64;
    let maxSize = outputDevice.bufferSizeRange?.max ?? 4096;

    if (inputDevice?.bufferSizeRange) {
        minSize = Math.max(minSize, inputDevice.bufferSizeRange.min);
        maxSize = Math.min(maxSize, inputDevice.bufferSizeRange.max);
    }

    return powerOf2Sizes.filter((size) => size >= minSize && size <= maxSize);
}

/**
 * Audio device choices. They take effect on Apply, since reopening the
 * streams interrupts the sound. An output selection of `null` follows the
 * system default output.
 */
export function AudioSettingsTab({ isActive }: AudioSettingsTabProps) {
    // Device cache from Rust
    const [deviceCache, setDeviceCache] = useState<DeviceCacheSnapshot | null>(
        null,
    );
    const [currentState, setCurrentState] = useState<CurrentAudioState | null>(
        null,
    );

    // Current selections (pending changes, not yet applied)
    const [selectedHostId, setSelectedHostId] = useState<string | null>(null);
    const [selectedOutputDeviceId, setSelectedOutputDeviceId] = useState<
        string | null
    >(null);
    const [selectedInputDeviceId, setSelectedInputDeviceId] = useState<
        string | null
    >(null);
    const [selectedSampleRate, setSelectedSampleRate] = useState<number | null>(
        null,
    );
    const [selectedBufferSize, setSelectedBufferSize] = useState<number | null>(
        null,
    );

    // UI state
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState<string | null>(null);
    const [warning, setWarning] = useState<string | null>(null);
    const [applying, setApplying] = useState(false);

    // Filter devices by selected host
    const outputDevicesForHost = useMemo(() => {
        if (!deviceCache || !selectedHostId) {
            return [];
        }
        const hostData = deviceCache.hosts.find(
            (h) => h.hostId === selectedHostId,
        );
        return hostData?.outputDevices ?? [];
    }, [deviceCache, selectedHostId]);

    const inputDevicesForHost = useMemo(() => {
        if (!deviceCache || !selectedHostId) {
            return [];
        }
        const hostData = deviceCache.hosts.find(
            (h) => h.hostId === selectedHostId,
        );
        return hostData?.inputDevices ?? [];
    }, [deviceCache, selectedHostId]);

    // Names of the output devices this tab has listed, so a chosen device
    // that is not connected keeps its name in the list.
    const seenOutputNames = useRef(new Map<string, string>());
    for (const host of deviceCache?.hosts ?? []) {
        for (const device of host.outputDevices) {
            seenOutputNames.current.set(device.id, device.name);
        }
    }
    const unlistedOutputDeviceId =
        selectedOutputDeviceId !== null &&
        !outputDevicesForHost.some((d) => d.id === selectedOutputDeviceId)
            ? selectedOutputDeviceId
            : null;

    const outputDeviceFollowDevice = useMemo(
        () => outputDevicesForHost.find((d) => d.isDefault) ?? null,
        [outputDevicesForHost],
    );

    // Get selected device objects
    const selectedOutputDevice = useMemo(
        () =>
            selectedOutputDeviceId === null
                ? outputDeviceFollowDevice
                : (outputDevicesForHost.find(
                      (d) => d.id === selectedOutputDeviceId,
                  ) ?? null),
        [
            outputDevicesForHost,
            selectedOutputDeviceId,
            outputDeviceFollowDevice,
        ],
    );

    // A chosen output that is not connected offers what the device playing in
    // its place allows, along with the settings already chosen for it.
    const playingOutputDevice = useMemo(
        () =>
            outputDevicesForHost.find(
                (d) => d.id === currentState?.outputDeviceId,
            ) ?? null,
        [outputDevicesForHost, currentState],
    );
    const capabilityOutputDevice = unlistedOutputDeviceId
        ? playingOutputDevice
        : selectedOutputDevice;
    const chosenForUnlisted =
        unlistedOutputDeviceId && currentState
            ? shownSettings(currentState)
            : null;

    const selectedInputDevice = useMemo(
        () =>
            inputDevicesForHost.find((d) => d.id === selectedInputDeviceId) ??
            null,
        [inputDevicesForHost, selectedInputDeviceId],
    );

    // Compute available sample rates and buffer sizes (JS-side logic)
    const chosenSampleRate = chosenForUnlisted?.sampleRate ?? null;
    const chosenBufferSize = chosenForUnlisted?.bufferSize ?? null;
    const availableSampleRates = useMemo(
        () =>
            withChosen(
                computeCommonSampleRates(
                    capabilityOutputDevice,
                    selectedInputDevice,
                ),
                chosenSampleRate,
            ),
        [capabilityOutputDevice, selectedInputDevice, chosenSampleRate],
    );

    const availableBufferSizes = useMemo(
        () =>
            withChosen(
                computeBufferSizes(capabilityOutputDevice, selectedInputDevice),
                chosenBufferSize,
            ),
        [capabilityOutputDevice, selectedInputDevice, chosenBufferSize],
    );

    // When the device list was last re-read, as `performance.now()`.
    const lastDeviceRefreshAt = useRef(-Infinity);

    // Load data from device cache
    const loadData = useCallback(async () => {
        try {
            setLoading(true);
            setError(null);

            await electronAPI.audio.refreshDeviceCache(true);
            lastDeviceRefreshAt.current = performance.now();
            const [cache, state] = await Promise.all([
                electronAPI.audio.getDeviceCache(),
                electronAPI.audio.getCurrentState(),
            ]);

            setDeviceCache(cache);
            setCurrentState(state);

            // The engine keeps reporting a device fallback in its state until
            // the streams are recreated, so surface it even when this panel
            // opens long after the fallback happened.
            setWarning(state.fallbackWarning ?? null);

            // Initialize selections from current state
            setSelectedHostId(state.hostId);
            setSelectedOutputDeviceId(state.chosenOutputDeviceId ?? null);
            setSelectedInputDeviceId(state.inputDeviceId ?? null);
            const shown = shownSettings(state);
            setSelectedSampleRate(shown.sampleRate);
            setSelectedBufferSize(shown.bufferSize);
        } catch (err) {
            setError(
                err instanceof Error ? err.message : 'Failed to load settings',
            );
        } finally {
            setLoading(false);
        }
    }, []);

    // Re-read the engine's state after the output changed under it. A
    // selection still at the previous state's value moves to the new one; one
    // the user changed stays.
    const currentStateRef = useRef<CurrentAudioState | null>(null);
    currentStateRef.current = currentState;
    const syncCurrentState = useCallback(async () => {
        const previous = currentStateRef.current;
        const [cache, state] = await Promise.all([
            electronAPI.audio.getDeviceCache(),
            electronAPI.audio.getCurrentState(),
        ]);
        setDeviceCache(cache);
        setCurrentState(state);
        setWarning(state.fallbackWarning ?? null);
        if (!previous) {
            return;
        }
        const follow =
            <T,>(was: T, now: T) =>
            (selected: T) =>
                selected === was ? now : selected;
        setSelectedHostId(follow<string | null>(previous.hostId, state.hostId));
        setSelectedOutputDeviceId(
            follow(
                previous.chosenOutputDeviceId ?? null,
                state.chosenOutputDeviceId ?? null,
            ),
        );
        setSelectedInputDeviceId(
            follow(previous.inputDeviceId ?? null, state.inputDeviceId ?? null),
        );
        const was = shownSettings(previous);
        const now = shownSettings(state);
        setSelectedSampleRate(
            follow<number | null>(was.sampleRate, now.sampleRate),
        );
        setSelectedBufferSize(follow(was.bufferSize, now.bufferSize));
    }, []);

    // When the host changes, reset device selections to defaults for that
    // host. On the current host, and when the device list is re-read,
    // selections stay: a chosen device that is not connected stays selected.
    const hostSelectionsResolved = useRef<string | null>(null);
    useEffect(() => {
        if (!deviceCache || !selectedHostId || !currentState) {
            return;
        }

        const hostData = deviceCache.hosts.find(
            (h) => h.hostId === selectedHostId,
        );
        if (!hostData || hostSelectionsResolved.current === selectedHostId) {
            return;
        }
        hostSelectionsResolved.current = selectedHostId;
        if (selectedHostId === currentState.hostId) {
            return;
        }

        // If current output device isn't in this host, follow its default
        const outputInHost = hostData.outputDevices.find(
            (d) => d.id === selectedOutputDeviceId,
        );
        if (!outputInHost) {
            setSelectedOutputDeviceId(null);
        }

        // If current input device isn't in this host, select none or default
        const inputInHost = hostData.inputDevices.find(
            (d) => d.id === selectedInputDeviceId,
        );
        if (!inputInHost && selectedInputDeviceId !== null) {
            setSelectedInputDeviceId(null);
        }
    }, [
        currentState,
        deviceCache,
        selectedHostId,
        selectedInputDeviceId,
        selectedOutputDeviceId,
    ]);

    // Update sample rate and buffer size when device selection changes
    useEffect(() => {
        // A selection the devices cannot run gives way to the engine's
        // defaults where they can, then to the output's own rate and the
        // lowest buffer size.
        if (
            selectedSampleRate === null ||
            !availableSampleRates.includes(selectedSampleRate)
        ) {
            const deviceDefault = capabilityOutputDevice?.sampleRate ?? null;
            const preferred = availableSampleRates.includes(
                PREFERRED_SAMPLE_RATE,
            )
                ? PREFERRED_SAMPLE_RATE
                : deviceDefault !== null &&
                    availableSampleRates.includes(deviceDefault)
                  ? deviceDefault
                  : availableSampleRates.length > 0
                    ? availableSampleRates[availableSampleRates.length - 1]
                    : null;
            setSelectedSampleRate(preferred);
        }

        if (
            selectedBufferSize === null ||
            !availableBufferSizes.includes(selectedBufferSize)
        ) {
            const preferred = availableBufferSizes.includes(
                PREFERRED_BUFFER_SIZE,
            )
                ? PREFERRED_BUFFER_SIZE
                : availableBufferSizes.length > 0
                  ? Math.min(...availableBufferSizes)
                  : null;
            setSelectedBufferSize(preferred);
        }
    }, [
        availableSampleRates,
        availableBufferSizes,
        selectedSampleRate,
        selectedBufferSize,
        capabilityOutputDevice,
    ]);

    // Re-read the device list without touching pending selections. Runs as
    // the pointer or focus reaches a device dropdown, ahead of the click or
    // key that opens it: a native select's popup lists the options present
    // when it opens. Re-reads at most every couple of seconds, since some
    // hosts (ALSA) take far longer to list than a pointer takes to cross the
    // dropdowns.
    const deviceRefreshInFlight = useRef(false);
    const refreshDeviceList = useCallback(async () => {
        if (
            deviceRefreshInFlight.current ||
            performance.now() - lastDeviceRefreshAt.current <
                DEVICE_REFRESH_INTERVAL_MS
        ) {
            return;
        }
        deviceRefreshInFlight.current = true;
        try {
            await electronAPI.audio.refreshDeviceCache();
            setDeviceCache(await electronAPI.audio.getDeviceCache());
        } catch (err) {
            setError(
                err instanceof Error
                    ? err.message
                    : 'Failed to refresh devices',
            );
        } finally {
            deviceRefreshInFlight.current = false;
            lastDeviceRefreshAt.current = performance.now();
        }
    }, []);

    // Load on active
    useEffect(() => {
        if (isActive) {
            void loadData();
        }
    }, [isActive, loadData]);

    // Listen for fallback warnings
    useEffect(() => {
        const unsubscribe = electronAPI.audio.onFallbackWarning(
            (msg: string) => {
                setWarning(msg);
            },
        );
        return unsubscribe;
    }, []);

    // The output device changed, went or came back: show what the engine
    // runs with now. Selections the user has not changed follow it; pending
    // edits stay.
    useEffect(() => {
        if (!isActive) {
            return;
        }
        return electronAPI.audio.onDeviceNotice(() => {
            void syncCurrentState();
        });
    }, [isActive, syncCurrentState]);

    // Apply configuration using recreateStreams
    const applyConfig = useCallback(async () => {
        if (!selectedSampleRate) {
            setError('Please select a sample rate');
            return;
        }

        try {
            setApplying(true);
            setError(null);
            setWarning(null);

            await electronAPI.audio.recreateStreams(
                selectedOutputDeviceId ?? undefined,
                selectedSampleRate,
                selectedBufferSize ?? undefined,
                selectedInputDeviceId ?? undefined,
                selectedHostId ?? undefined,
            );

            // Reload state to get actual applied values
            const newState = await electronAPI.audio.getCurrentState();
            setCurrentState(newState);

            // Update selections to match what was actually applied
            setSelectedHostId(newState.hostId);
            setSelectedOutputDeviceId(newState.chosenOutputDeviceId ?? null);
            setSelectedInputDeviceId(newState.inputDeviceId ?? null);
            setSelectedSampleRate(newState.sampleRate);
            setSelectedBufferSize(newState.bufferSize ?? null);
        } catch (err) {
            setError(
                err instanceof Error
                    ? err.message
                    : 'Failed to apply configuration',
            );
        } finally {
            setApplying(false);
        }
    }, [
        selectedHostId,
        selectedOutputDeviceId,
        selectedSampleRate,
        selectedBufferSize,
        selectedInputDeviceId,
    ]);

    const isDirty = useMemo(() => {
        if (!currentState) {
            return false;
        }
        return (
            selectedHostId !== currentState.hostId ||
            selectedOutputDeviceId !==
                (currentState.chosenOutputDeviceId ?? null) ||
            selectedInputDeviceId !== (currentState.inputDeviceId ?? null) ||
            selectedSampleRate !== shownSettings(currentState).sampleRate ||
            selectedBufferSize !== shownSettings(currentState).bufferSize
        );
    }, [
        currentState,
        selectedHostId,
        selectedOutputDeviceId,
        selectedInputDeviceId,
        selectedSampleRate,
        selectedBufferSize,
    ]);

    // Only show hosts that have at least one available device
    const hosts =
        deviceCache?.hosts
            .filter(
                (h) => h.outputDevices.length > 0 || h.inputDevices.length > 0,
            )
            .map((h) => ({ id: h.hostId, name: h.hostName })) ?? [];

    return (
        <div className="settings-tab-content">
            {error && <div className="settings-tab-error">{error}</div>}

            {warning && <div className="settings-tab-warning">{warning}</div>}

            {loading ? (
                <div className="settings-tab-loading">Loading devices...</div>
            ) : (
                <>
                    {/* Audio Host */}
                    <div className="settings-section">
                        <h3>Audio Host</h3>
                        <select
                            className="device-select"
                            onPointerEnter={refreshDeviceList}
                            onFocus={refreshDeviceList}
                            value={selectedHostId || ''}
                            onChange={(e) =>
                                setSelectedHostId(e.target.value || null)
                            }
                        >
                            {hosts.map((h) => (
                                <option key={h.id} value={h.id}>
                                    {h.name}
                                </option>
                            ))}
                        </select>
                    </div>

                    {/* Output Device */}
                    <div className="settings-section">
                        <h3>Output Device</h3>
                        <select
                            className="device-select"
                            onPointerEnter={refreshDeviceList}
                            onFocus={refreshDeviceList}
                            value={selectedOutputDeviceId || ''}
                            onChange={(e) =>
                                setSelectedOutputDeviceId(
                                    e.target.value || null,
                                )
                            }
                        >
                            <option value="">
                                System Default
                                {outputDeviceFollowDevice
                                    ? ` (${outputDeviceFollowDevice.name})`
                                    : ''}
                            </option>
                            <optgroup label="Devices">
                                {unlistedOutputDeviceId && (
                                    <option value={unlistedOutputDeviceId}>
                                        {seenOutputNames.current.get(
                                            unlistedOutputDeviceId,
                                        ) ?? 'Chosen device'}{' '}
                                        (not connected)
                                    </option>
                                )}
                                {outputDevicesForHost.map((d) => (
                                    <option key={d.id} value={d.id}>
                                        {d.name} ({d.outputChannels} ch)
                                    </option>
                                ))}
                            </optgroup>
                        </select>
                        {selectedOutputDevice && (
                            <div className="device-info">
                                {selectedOutputDevice.sampleRate} Hz,{' '}
                                {selectedOutputDevice.outputChannels} channels
                            </div>
                        )}
                    </div>

                    {/* Input Device */}
                    <div className="settings-section">
                        <h3>Input Device</h3>
                        <select
                            className="device-select"
                            onPointerEnter={refreshDeviceList}
                            onFocus={refreshDeviceList}
                            value={selectedInputDeviceId || ''}
                            onChange={(e) =>
                                setSelectedInputDeviceId(e.target.value || null)
                            }
                        >
                            <option value="">None (No Input)</option>
                            {inputDevicesForHost.map((d) => (
                                <option key={d.id} value={d.id}>
                                    {d.name} ({d.inputChannels} ch)
                                    {d.isDefault ? ' (Default)' : ''}
                                </option>
                            ))}
                        </select>
                        {selectedInputDevice && (
                            <div className="device-info">
                                {selectedInputDevice.sampleRate} Hz,{' '}
                                {selectedInputDevice.inputChannels} channels
                            </div>
                        )}
                    </div>

                    {/* Sample Rate */}
                    <div className="settings-section">
                        <h3>Sample Rate</h3>
                        <select
                            className="device-select"
                            value={selectedSampleRate || ''}
                            onChange={(e) =>
                                setSelectedSampleRate(
                                    Number(e.target.value) || null,
                                )
                            }
                            disabled={availableSampleRates.length === 0}
                        >
                            {availableSampleRates.length === 0 ? (
                                <option value="">Select devices first</option>
                            ) : (
                                availableSampleRates.map((rate) => (
                                    <option key={rate} value={rate}>
                                        {rate} Hz
                                    </option>
                                ))
                            )}
                        </select>
                    </div>

                    {/* Buffer Size */}
                    <div className="settings-section">
                        <h3>Buffer Size</h3>
                        <select
                            className="device-select"
                            value={selectedBufferSize || ''}
                            onChange={(e) =>
                                setSelectedBufferSize(
                                    Number(e.target.value) || null,
                                )
                            }
                            disabled={availableBufferSizes.length === 0}
                        >
                            {availableBufferSizes.length === 0 ? (
                                <option value="">Select devices first</option>
                            ) : (
                                availableBufferSizes.map((size) => (
                                    <option key={size} value={size}>
                                        {size} samples (~
                                        {(
                                            (size /
                                                (selectedSampleRate || 48000)) *
                                            1000
                                        ).toFixed(1)}{' '}
                                        ms)
                                    </option>
                                ))
                            )}
                        </select>
                    </div>

                    <div className="settings-section settings-row">
                        <span className="action-spacer" />
                        <button
                            className="btn btn-primary"
                            onClick={applyConfig}
                            disabled={applying || !isDirty}
                        >
                            {applying ? 'Applying…' : 'Apply'}
                        </button>
                    </div>
                </>
            )}
        </div>
    );
}
