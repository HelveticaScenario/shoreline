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

function loaded(state: CurrentAudioState = engine()): AudioSettingsState {
    return audioSettingsReducer(INITIAL_AUDIO_SETTINGS, {
        cache: CACHE,
        engine: state,
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
});
