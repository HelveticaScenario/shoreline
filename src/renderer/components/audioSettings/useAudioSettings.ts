import { useEffect, useEffectEvent, useReducer, useRef, useState } from 'react';
import electronAPI from '../../electronAPI';
import type {
    CurrentAudioState,
    DeviceCacheSnapshot,
} from '../../../shared/ipcTypes';
import {
    INITIAL_AUDIO_SETTINGS,
    audioSettingsReducer,
} from './audioSettingsState';
import {
    deviceOptions,
    sameSelection,
    shownSelection,
    type AudioSelection,
} from './deviceOptions';

const DEVICE_REFRESH_INTERVAL_MS = 2000;

function errorMessage(err: unknown, fallback: string): string {
    return err instanceof Error ? err.message : fallback;
}

/**
 * The device list and what the engine runs with. The engine keeps reporting a
 * device fallback in its state until the streams are recreated, so `warning`
 * holds one that happened long before the read.
 */
async function readDevicesAndEngine(): Promise<{
    cache: DeviceCacheSnapshot;
    engine: CurrentAudioState;
    warning: string | null;
}> {
    const [cache, engine] = await Promise.all([
        electronAPI.audio.getDeviceCache(),
        electronAPI.audio.getCurrentState(),
    ]);
    return { cache, engine, warning: engine.fallbackWarning ?? null };
}

/** Reopens the streams with `selection` and returns what the engine then runs with. */
async function applySelection(
    selection: AudioSelection,
    sampleRate: number,
): Promise<CurrentAudioState> {
    await electronAPI.audio.recreateStreams(
        selection.outputDeviceId ?? undefined,
        sampleRate,
        selection.bufferSize ?? undefined,
        selection.inputDeviceId ?? undefined,
        selection.hostId ?? undefined,
    );
    return electronAPI.audio.getCurrentState();
}

/**
 * The audio tab's devices, pending selection and actions. `isActive` is
 * whether the tab is showing: it loads on becoming active and follows the
 * engine's output while it is.
 */
export function useAudioSettings(isActive: boolean) {
    const [state, dispatch] = useReducer(
        audioSettingsReducer,
        INITIAL_AUDIO_SETTINGS,
    );
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState<string | null>(null);
    const [warning, setWarning] = useState<string | null>(null);
    const [applying, setApplying] = useState(false);

    // When the device list was last re-read, as `performance.now()`.
    const lastDeviceRefreshAt = useRef(-Infinity);
    const deviceRefreshInFlight = useRef(false);

    const { deviceCache, engine, selection } = state;
    const options = deviceOptions(deviceCache, engine, selection);

    // Each time the tab becomes active it shows as loading until the
    // engine's state has been read afresh.
    const [wasActive, setWasActive] = useState(false);
    if (isActive !== wasActive) {
        setWasActive(isActive);
        if (isActive) {
            setLoading(true);
            setError(null);
        }
    }

    // Re-read the engine's state after the output changed under it. A
    // selection still at the previous state's value moves to the new one; one
    // the user changed stays.
    const syncEngine = useEffectEvent(async () => {
        const previous = engine;
        const read = await readDevicesAndEngine();
        dispatch({
            cache: read.cache,
            engine: read.engine,
            previous,
            type: 'engineChanged',
        });
        setWarning(read.warning);
    });

    useEffect(() => {
        if (!isActive) {
            return;
        }
        electronAPI.audio
            .refreshDeviceCache(true)
            .then(() => {
                lastDeviceRefreshAt.current = performance.now();
                return readDevicesAndEngine();
            })
            .then((read) => {
                dispatch({
                    cache: read.cache,
                    engine: read.engine,
                    type: 'loaded',
                });
                setWarning(read.warning);
            })
            .catch((err) => {
                setError(errorMessage(err, 'Failed to load settings'));
            })
            .then(() => setLoading(false))
            .catch(console.error);
    }, [isActive]);

    useEffect(() => electronAPI.audio.onFallbackWarning(setWarning), []);

    // The output device changed, went or came back: show what the engine
    // runs with now.
    useEffect(() => {
        if (!isActive) {
            return;
        }
        return electronAPI.audio.onDeviceNotice(() => {
            void syncEngine();
        });
    }, [isActive]);

    // Re-read the device list without touching pending selections. Runs as
    // the pointer or focus reaches a device dropdown, ahead of the click or
    // key that opens it: a native select's popup lists the options present
    // when it opens. Re-reads at most every couple of seconds, since some
    // hosts (ALSA) take far longer to list than a pointer takes to cross the
    // dropdowns.
    const refreshDeviceList = async () => {
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
            const cache = await electronAPI.audio.getDeviceCache();
            dispatch({ cache, type: 'devicesListed' });
        } catch (err) {
            setError(errorMessage(err, 'Failed to refresh devices'));
        }
        deviceRefreshInFlight.current = false;
        lastDeviceRefreshAt.current = performance.now();
    };

    const applyConfig = async () => {
        const { sampleRate } = selection;
        if (!sampleRate) {
            setError('Please select a sample rate');
            return;
        }
        setApplying(true);
        setError(null);
        setWarning(null);
        try {
            const applied = await applySelection(selection, sampleRate);
            dispatch({ engine: applied, type: 'applied' });
        } catch (err) {
            setError(errorMessage(err, 'Failed to apply configuration'));
        }
        setApplying(false);
    };

    const { unlistedOutputDeviceId } = options;

    return {
        applyConfig,
        applying,
        error,
        /** The hosts with at least one device. */
        hosts:
            deviceCache?.hosts.filter(
                (h) => h.outputDevices.length > 0 || h.inputDevices.length > 0,
            ) ?? [],
        isDirty:
            engine !== null &&
            !sameSelection(selection, shownSelection(engine)),
        loading,
        options,
        refreshDeviceList,
        select: (patch: Partial<AudioSelection>) =>
            dispatch({ patch, type: 'selected' }),
        selection,
        /** The selected output's last listed name, when the host does not list it. */
        unlistedOutputName: unlistedOutputDeviceId
            ? (state.outputNames.get(unlistedOutputDeviceId) ?? 'Chosen device')
            : null,
        warning,
    };
}
