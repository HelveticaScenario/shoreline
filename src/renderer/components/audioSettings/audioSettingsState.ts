import type {
    CurrentAudioState,
    DeviceCacheSnapshot,
} from '../../../shared/ipcTypes';
import {
    EMPTY_SELECTION,
    appliedSelection,
    followSelection,
    patchSelection,
    shownSelection,
    type AudioSelection,
} from './audioSelection';
import { deviceOptions, runnableSettings } from './deviceOptions';

export interface AudioSettingsState {
    deviceCache: DeviceCacheSnapshot | null;
    /** What the engine runs with. */
    engine: CurrentAudioState | null;
    selection: AudioSelection;
    /**
     * Names of the output devices listed so far, so a chosen device that is
     * not connected keeps its name in the list.
     */
    outputNames: ReadonlyMap<string, string>;
    /** The host whose device selections are settled. */
    resolvedHostId: string | null;
    /** Whether the tab is showing. */
    active: boolean;
    /**
     * Counts the tab's activations and deactivations. A load carries the
     * count it started at, and its result lands only while that is still the
     * count and the load is still awaited.
     */
    loadRun: number;
    /** Whether the engine's state is still being read for this activation. */
    loading: boolean;
    applying: boolean;
    error: string | null;
    warning: string | null;
}

export type AudioSettingsAction =
    | { type: 'activeChanged'; active: boolean }
    | {
          type: 'loaded';
          run: number;
          cache: DeviceCacheSnapshot;
          engine: CurrentAudioState;
      }
    | { type: 'loadFailed'; run: number; message: string }
    /** The output changed under the engine. */
    | {
          type: 'engineChanged';
          cache: DeviceCacheSnapshot;
          engine: CurrentAudioState;
      }
    | { type: 'devicesListed'; cache: DeviceCacheSnapshot }
    | { type: 'applyStarted' }
    | { type: 'applied'; engine: CurrentAudioState }
    | { type: 'applyFailed'; message: string }
    | { type: 'selected'; patch: Partial<AudioSelection> }
    /** The engine reported a device fallback. */
    | { type: 'warned'; warning: string }
    | { type: 'failed'; message: string };

export const INITIAL_AUDIO_SETTINGS: AudioSettingsState = {
    active: false,
    applying: false,
    deviceCache: null,
    engine: null,
    error: null,
    loadRun: 0,
    loading: true,
    outputNames: new Map(),
    resolvedHostId: null,
    selection: EMPTY_SELECTION,
    warning: null,
};

function withOutputNames(
    names: ReadonlyMap<string, string>,
    cache: DeviceCacheSnapshot,
): ReadonlyMap<string, string> {
    const next = new Map(names);
    for (const host of cache.hosts) {
        for (const device of host.outputDevices) {
            next.set(device.id, device.name);
        }
    }
    return next;
}

/**
 * The device selections a newly selected host cannot keep, cleared. On the
 * engine's host, and when the device list is re-read, selections stay: a
 * chosen device that is not connected stays selected.
 */
function hostDefaults(state: AudioSettingsState): {
    resolvedHostId: string | null;
    patch: Partial<AudioSelection>;
} {
    const { deviceCache, engine, selection, resolvedHostId } = state;
    const unchanged = { patch: {}, resolvedHostId };
    if (!deviceCache || !selection.hostId || !engine) {
        return unchanged;
    }
    const host = deviceCache.hosts.find((h) => h.hostId === selection.hostId);
    if (!host || resolvedHostId === selection.hostId) {
        return unchanged;
    }

    const patch: Partial<AudioSelection> = {};
    if (selection.hostId !== engine.hostId) {
        if (
            !host.outputDevices.some((d) => d.id === selection.outputDeviceId)
        ) {
            patch.outputDeviceId = null;
        }
        if (!host.inputDevices.some((d) => d.id === selection.inputDeviceId)) {
            patch.inputDeviceId = null;
        }
    }
    return { patch, resolvedHostId: selection.hostId };
}

/**
 * `state` with its selection brought to one its host and devices can run.
 * The host's defaults and the runnable settings are both read from the same
 * state, then again from the result until it holds still.
 */
function settle(state: AudioSettingsState): AudioSettingsState {
    for (;;) {
        const { patch, resolvedHostId } = hostDefaults(state);
        const selection = patchSelection(state.selection, {
            ...patch,
            ...runnableSettings(
                state.selection,
                deviceOptions(state.deviceCache, state.engine, state.selection),
            ),
        });
        if (
            selection === state.selection &&
            resolvedHostId === state.resolvedHostId
        ) {
            return state;
        }
        state = { ...state, resolvedHostId, selection };
    }
}

function withDeviceCache(
    state: AudioSettingsState,
    cache: DeviceCacheSnapshot,
): AudioSettingsState {
    return {
        ...state,
        deviceCache: cache,
        outputNames: withOutputNames(state.outputNames, cache),
    };
}

/** Whether the load started at `run` is the one `state` waits on. */
function awaitsLoad(state: AudioSettingsState, run: number): boolean {
    return state.loading && state.loadRun === run;
}

export function audioSettingsReducer(
    state: AudioSettingsState,
    action: AudioSettingsAction,
): AudioSettingsState {
    switch (action.type) {
        case 'activeChanged':
            if (action.active === state.active) {
                return state;
            }
            return {
                ...state,
                active: action.active,
                loadRun: state.loadRun + 1,
                ...(action.active ? { error: null, loading: true } : {}),
            };
        case 'loaded':
            if (!awaitsLoad(state, action.run)) {
                return state;
            }
            return settle({
                ...withDeviceCache(state, action.cache),
                engine: action.engine,
                loading: false,
                selection: shownSelection(action.engine),
                warning: action.engine.fallbackWarning ?? null,
            });
        case 'loadFailed':
            if (!awaitsLoad(state, action.run)) {
                return state;
            }
            return { ...state, error: action.message, loading: false };
        case 'engineChanged':
            return settle({
                ...withDeviceCache(state, action.cache),
                engine: action.engine,
                // A choice still at the value the engine left moves to its
                // new one; one the user changed stays.
                selection: state.engine
                    ? followSelection(
                          state.selection,
                          shownSelection(state.engine),
                          shownSelection(action.engine),
                      )
                    : state.selection,
                warning: action.engine.fallbackWarning ?? null,
            });
        case 'devicesListed':
            return settle(withDeviceCache(state, action.cache));
        case 'applyStarted':
            return { ...state, applying: true, error: null, warning: null };
        case 'applied':
            return settle({
                ...state,
                applying: false,
                engine: action.engine,
                selection: appliedSelection(action.engine),
            });
        case 'applyFailed':
            return { ...state, applying: false, error: action.message };
        case 'selected':
            return settle({
                ...state,
                selection: patchSelection(state.selection, action.patch),
            });
        case 'warned':
            return { ...state, warning: action.warning };
        case 'failed':
            return { ...state, error: action.message };
    }
}
