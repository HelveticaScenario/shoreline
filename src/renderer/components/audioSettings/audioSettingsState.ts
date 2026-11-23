import type {
    CurrentAudioState,
    DeviceCacheSnapshot,
} from '../../../shared/ipcTypes';
import {
    EMPTY_SELECTION,
    appliedSelection,
    deviceOptions,
    followSelection,
    patchSelection,
    runnableSettings,
    shownSelection,
    type AudioSelection,
} from './deviceOptions';

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
}

export type AudioSettingsAction =
    | { type: 'loaded'; cache: DeviceCacheSnapshot; engine: CurrentAudioState }
    | {
          /** The output changed under the engine; `previous` is the state it left. */
          type: 'engineChanged';
          cache: DeviceCacheSnapshot;
          engine: CurrentAudioState;
          previous: CurrentAudioState | null;
      }
    | { type: 'devicesListed'; cache: DeviceCacheSnapshot }
    | { type: 'applied'; engine: CurrentAudioState }
    | { type: 'selected'; patch: Partial<AudioSelection> };

export const INITIAL_AUDIO_SETTINGS: AudioSettingsState = {
    deviceCache: null,
    engine: null,
    outputNames: new Map(),
    resolvedHostId: null,
    selection: EMPTY_SELECTION,
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

export function audioSettingsReducer(
    state: AudioSettingsState,
    action: AudioSettingsAction,
): AudioSettingsState {
    switch (action.type) {
        case 'loaded':
            return settle({
                ...withDeviceCache(state, action.cache),
                engine: action.engine,
                selection: shownSelection(action.engine),
            });
        case 'engineChanged':
            return settle({
                ...withDeviceCache(state, action.cache),
                engine: action.engine,
                selection: action.previous
                    ? followSelection(
                          state.selection,
                          shownSelection(action.previous),
                          shownSelection(action.engine),
                      )
                    : state.selection,
            });
        case 'devicesListed':
            return settle(withDeviceCache(state, action.cache));
        case 'applied':
            return settle({
                ...state,
                engine: action.engine,
                selection: appliedSelection(action.engine),
            });
        case 'selected':
            return settle({
                ...state,
                selection: patchSelection(state.selection, action.patch),
            });
    }
}
