import { describe, expect, test } from 'vitest';
import type {
    AudioDeviceInfo,
    CurrentAudioState,
    DeviceCacheSnapshot,
} from '../../../../shared/ipcTypes';
import {
    INITIAL_AUDIO_SETTINGS,
    audioSettingsReducer,
    type AudioSettingsState,
} from '../audioSettingsState';

function device(
    id: string,
    overrides: Partial<AudioDeviceInfo> = {},
): AudioDeviceInfo {
    return {
        bufferSizeRange: { max: 2048, min: 64 },
        followsDefault: false,
        hostId: 'core',
        id,
        inputChannels: 0,
        isDefault: false,
        name: id,
        outputChannels: 2,
        sampleRate: 48_000,
        supportedSampleRates: [44_100, 48_000, 96_000],
        ...overrides,
    };
}

const CACHE: DeviceCacheSnapshot = {
    hosts: [
        {
            hostId: 'core',
            hostName: 'Core',
            inputDevices: [device('mic', { inputChannels: 1 })],
            outputDevices: [
                device('speakers', { isDefault: true }),
                device('interface'),
            ],
        },
        {
            hostId: 'jack',
            hostName: 'Jack',
            inputDevices: [],
            outputDevices: [
                device('jack-out', {
                    bufferSizeRange: { max: 4096, min: 512 },
                    hostId: 'jack',
                    isDefault: true,
                    sampleRate: 44_100,
                    supportedSampleRates: [44_100],
                }),
            ],
        },
    ],
};

function engine(overrides: Partial<CurrentAudioState> = {}): CurrentAudioState {
    return {
        bufferSize: 256,
        chosenSampleRate: 48_000,
        hostId: 'core',
        inputChannels: 0,
        outputChannels: 2,
        outputDeviceId: 'speakers',
        sampleRate: 48_000,
        ...overrides,
    };
}

function activated(
    state: AudioSettingsState = INITIAL_AUDIO_SETTINGS,
): AudioSettingsState {
    return audioSettingsReducer(state, { active: true, type: 'activeChanged' });
}

function deactivated(state: AudioSettingsState): AudioSettingsState {
    return audioSettingsReducer(state, {
        active: false,
        type: 'activeChanged',
    });
}

function loaded(state: CurrentAudioState = engine()): AudioSettingsState {
    const loading = activated();
    return audioSettingsReducer(loading, {
        cache: CACHE,
        engine: state,
        run: loading.loadRun,
        type: 'loaded',
    });
}

describe('audioSettingsReducer', () => {
    test('loading shows what the engine runs with', () => {
        expect(loaded().selection).toEqual({
            bufferSize: 256,
            hostId: 'core',
            inputDeviceId: null,
            outputDeviceId: null,
            sampleRate: 48_000,
        });
    });

    test('a setting the selected devices cannot run falls back', () => {
        const state = audioSettingsReducer(loaded(), {
            patch: { hostId: 'jack' },
            type: 'selected',
        });
        expect(state.selection).toMatchObject({
            bufferSize: 512,
            hostId: 'jack',
            sampleRate: 44_100,
        });
    });

    test('another host drops the devices it does not list', () => {
        const chosen = audioSettingsReducer(loaded(), {
            patch: { inputDeviceId: 'mic', outputDeviceId: 'interface' },
            type: 'selected',
        });
        const state = audioSettingsReducer(chosen, {
            patch: { hostId: 'jack' },
            type: 'selected',
        });
        expect(state.selection).toMatchObject({
            inputDeviceId: null,
            outputDeviceId: null,
        });
    });

    test('a device list re-read keeps a chosen output that is not connected', () => {
        const chosen = audioSettingsReducer(loaded(), {
            patch: { outputDeviceId: 'interface' },
            type: 'selected',
        });
        const unplugged: DeviceCacheSnapshot = {
            hosts: CACHE.hosts.map((host) => ({
                ...host,
                outputDevices: host.outputDevices.filter(
                    (d) => d.id !== 'interface',
                ),
            })),
        };
        const state = audioSettingsReducer(chosen, {
            cache: unplugged,
            type: 'devicesListed',
        });
        expect(state.selection.outputDeviceId).toBe('interface');
        expect(state.outputNames.get('interface')).toBe('interface');
    });

    test('an engine change moves the choices the user left alone', () => {
        const edited = audioSettingsReducer(loaded(), {
            patch: { bufferSize: 512 },
            type: 'selected',
        });
        const state = audioSettingsReducer(edited, {
            cache: CACHE,
            engine: engine({ chosenSampleRate: 96_000, sampleRate: 96_000 }),
            type: 'engineChanged',
        });
        expect(state.selection).toMatchObject({
            bufferSize: 512,
            sampleRate: 96_000,
        });
    });

    test('successive engine changes each follow from the one before', () => {
        const first = audioSettingsReducer(loaded(), {
            cache: CACHE,
            engine: engine({ chosenSampleRate: 96_000, sampleRate: 96_000 }),
            type: 'engineChanged',
        });
        const second = audioSettingsReducer(first, {
            cache: CACHE,
            engine: engine({ chosenSampleRate: 44_100, sampleRate: 44_100 }),
            type: 'engineChanged',
        });
        expect(second.selection.sampleRate).toBe(44_100);
    });

    test('applying shows the settings the streams opened with', () => {
        const state = audioSettingsReducer(loaded(), {
            engine: engine({
                bufferSize: 1024,
                chosenOutputDeviceId: 'interface',
                outputDeviceId: 'interface',
                sampleRate: 44_100,
            }),
            type: 'applied',
        });
        expect(state.selection).toMatchObject({
            bufferSize: 1024,
            outputDeviceId: 'interface',
            sampleRate: 44_100,
        });
    });

    test('the tab loads from each activation until its result lands', () => {
        const loading = activated();
        expect(loading.loading).toBe(true);
        const state = audioSettingsReducer(loading, {
            cache: CACHE,
            engine: engine({ fallbackWarning: 'Playing on the speakers' }),
            run: loading.loadRun,
            type: 'loaded',
        });
        expect(state).toMatchObject({
            loading: false,
            warning: 'Playing on the speakers',
        });
        expect(activated(deactivated(state))).toMatchObject({
            error: null,
            loading: true,
        });
    });

    test('a failed load shows its error and ends loading', () => {
        const loading = activated();
        const state = audioSettingsReducer(loading, {
            message: 'No hosts',
            run: loading.loadRun,
            type: 'loadFailed',
        });
        expect(state).toMatchObject({ error: 'No hosts', loading: false });
        expect(activated(deactivated(state)).error).toBeNull();
    });

    test('a load the tab has restarted lands nothing', () => {
        const first = activated();
        const second = activated(deactivated(first));
        const stale = {
            cache: CACHE,
            engine: engine(),
            run: first.loadRun,
            type: 'loaded',
        } as const;
        expect(audioSettingsReducer(second, stale)).toBe(second);
        expect(
            audioSettingsReducer(second, {
                message: 'Timed out',
                run: first.loadRun,
                type: 'loadFailed',
            }),
        ).toBe(second);

        const current = audioSettingsReducer(second, {
            ...stale,
            run: second.loadRun,
        });
        const edited = audioSettingsReducer(current, {
            patch: { bufferSize: 512, outputDeviceId: 'interface' },
            type: 'selected',
        });
        expect(audioSettingsReducer(edited, stale)).toBe(edited);
    });

    test('a load lands once', () => {
        const state = loaded();
        const edited = audioSettingsReducer(state, {
            patch: { bufferSize: 512 },
            type: 'selected',
        });
        expect(
            audioSettingsReducer(edited, {
                cache: CACHE,
                engine: engine(),
                run: edited.loadRun,
                type: 'loaded',
            }),
        ).toBe(edited);
    });

    test('a load the tab has left lands nothing', () => {
        const loading = activated();
        const left = deactivated(loading);
        expect(
            audioSettingsReducer(left, {
                cache: CACHE,
                engine: engine(),
                run: loading.loadRun,
                type: 'loaded',
            }),
        ).toBe(left);
    });

    test('applying clears the messages and ends on either outcome', () => {
        const warned = audioSettingsReducer(
            audioSettingsReducer(loaded(), {
                type: 'warned',
                warning: 'Playing on the speakers',
            }),
            { message: 'Failed to refresh devices', type: 'failed' },
        );
        expect(warned).toMatchObject({
            error: 'Failed to refresh devices',
            warning: 'Playing on the speakers',
        });

        const applying = audioSettingsReducer(warned, { type: 'applyStarted' });
        expect(applying).toMatchObject({
            applying: true,
            error: null,
            warning: null,
        });
        expect(
            audioSettingsReducer(applying, {
                engine: engine(),
                type: 'applied',
            }),
        ).toMatchObject({ applying: false, error: null });
        expect(
            audioSettingsReducer(applying, {
                message: 'Device busy',
                type: 'applyFailed',
            }),
        ).toMatchObject({ applying: false, error: 'Device busy' });
    });

    test('an engine change shows the fallback it reports', () => {
        const state = audioSettingsReducer(loaded(), {
            cache: CACHE,
            engine: engine({ fallbackWarning: 'Playing on the speakers' }),
            type: 'engineChanged',
        });
        expect(state.warning).toBe('Playing on the speakers');
        expect(
            audioSettingsReducer(state, {
                cache: CACHE,
                engine: engine(),
                type: 'engineChanged',
            }).warning,
        ).toBeNull();
    });
});
