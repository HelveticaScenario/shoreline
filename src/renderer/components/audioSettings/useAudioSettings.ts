import { useEffect, useReducer, useRef } from 'react';
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
    sameSelection,
    shownSelection,
    type AudioSelection,
} from './audioSelection';
import { deviceOptions } from './deviceOptions';

const DEVICE_REFRESH_INTERVAL_MS = 2000;

function errorMessage(err: unknown, fallback: string): string {
    return err instanceof Error ? err.message : fallback;
}

/**
 * The device list and what the engine runs with. The engine reports a device
 * fallback in its state until the streams are recreated, so its
 * `fallbackWarning` can be one from long before the read.
 */
async function readDevicesAndEngine(): Promise<{
    cache: DeviceCacheSnapshot;
    engine: CurrentAudioState;
}> {
    const [cache, engine] = await Promise.all([
        electronAPI.audio.getDeviceCache(),
        electronAPI.audio.getCurrentState(),
    ]);
    return { cache, engine };
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
    // When the device list was last re-read, as `performance.now()`.
    const lastDeviceRefreshAt = useRef(-Infinity);
    const deviceRefreshInFlight = useRef(false);

    const { deviceCache, engine, loadRun, selection } = state;
    const options = deviceOptions(deviceCache, engine, selection);

    // Each time the tab becomes active it shows as loading until the
    // engine's state has been read afresh.
    if (isActive !== state.active) {
        dispatch({ active: isActive, type: 'activeChanged' });
    }

    // The reducer drops the result of a load the tab has since left or
    // restarted, so a slow device refresh cannot land over a later one.
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
                dispatch({ ...read, run: loadRun, type: 'loaded' });
            })
            .catch((err) => {
                dispatch({
                    message: errorMessage(err, 'Failed to load settings'),
                    run: loadRun,
                    type: 'loadFailed',
                });
            });
    }, [isActive, loadRun]);

    useEffect(
        () =>
            electronAPI.audio.onFallbackWarning((warning) =>
                dispatch({ type: 'warned', warning }),
            ),
        [],
    );

    // The output device changed, went or came back: show what the engine
    // runs with now.
    useEffect(() => {
        if (!isActive) {
            return;
        }
        return electronAPI.audio.onDeviceNotice(() => {
            readDevicesAndEngine()
                .then((read) => {
                    dispatch({ ...read, type: 'engineChanged' });
                })
                .catch((err) => {
                    dispatch({
                        message: errorMessage(
                            err,
                            'Failed to read audio state',
                        ),
                        type: 'failed',
                    });
                });
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
            dispatch({
                message: errorMessage(err, 'Failed to refresh devices'),
                type: 'failed',
            });
        }
        deviceRefreshInFlight.current = false;
        lastDeviceRefreshAt.current = performance.now();
    };

    const applyConfig = async () => {
        const { sampleRate } = selection;
        if (!sampleRate) {
            dispatch({
                message: 'Please select a sample rate',
                type: 'failed',
            });
            return;
        }
        dispatch({ type: 'applyStarted' });
        try {
            const applied = await applySelection(selection, sampleRate);
            dispatch({ engine: applied, type: 'applied' });
        } catch (err) {
            dispatch({
                message: errorMessage(err, 'Failed to apply configuration'),
                type: 'applyFailed',
            });
        }
    };

    const { unlistedOutputDeviceId } = options;

    return {
        applyConfig,
        applying: state.applying,
        error: state.error,
        /** The hosts with at least one device. */
        hosts:
            deviceCache?.hosts.filter(
                (h) => h.outputDevices.length > 0 || h.inputDevices.length > 0,
            ) ?? [],
        isDirty:
            engine !== null &&
            !sameSelection(selection, shownSelection(engine)),
        loading: state.loading,
        options,
        refreshDeviceList,
        select: (patch: Partial<AudioSelection>) =>
            dispatch({ patch, type: 'selected' }),
        selection,
        /** The selected output's last listed name, when the host does not list it. */
        unlistedOutputName: unlistedOutputDeviceId
            ? (state.outputNames.get(unlistedOutputDeviceId) ?? 'Chosen device')
            : null,
        warning: state.warning,
    };
}
