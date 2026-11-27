import type { AssetKind } from '../../shared/assetFiles';
import type { AssetParamTypeName } from '../../shared/dsl/assetParamTypes';
import {
    isDx7VoiceRef,
    selectVoice,
    splitVoice,
    type SyxVoice,
} from '../../shared/dx7Voice';
import { assetPath } from './assetPath';

/** What the engine reports about a decoded WAV or media audio track. */
export interface LoadedWav {
    channels: number;
    frameCount: number;
    path: string;
    sampleRate: number;
    duration: number;
    bitDepth: number;
    pitch?: number | null;
    playback?: string | null;
    bpm?: number | null;
    beats?: number | null;
    timeSignature?: { num: number; den: number } | null;
    barCount?: number | null;
    loops: Array<{ loopType: string; start: number; end: number }>;
    cuePoints: Array<{ position: number; label: string }>;
    mtime: number;
}

/** The host calls that read a patch's files, synchronously. */
export interface AssetLoaders {
    /** Decodes a WAV file named by a workspace or builtin path. */
    wav: (path: string) => LoadedWav;
    /** The DX7 voices of a `.syx` file named by a workspace or builtin path. */
    syx: (path: string) => SyxVoice[];
    /**
     * Decodes the audio track of a workspace media file, or returns null when
     * it has none. The returned `path` names the decoded audio for a `Wav`.
     */
    mediaAudio: (path: string) => LoadedWav | null;
}

const LOADER_NAMES: Record<keyof AssetLoaders, string> = {
    wav: 'WAV',
    syx: '.syx',
    mediaAudio: 'media audio',
};

/** The error for a host that gives no `loader`, naming `fn`. */
export function loaderUnavailable(
    fn: string,
    loader: keyof AssetLoaders,
): Error {
    return new Error(`${fn}: ${LOADER_NAMES[loader]} loading is not available`);
}

/** Runs the `loader` host call on `path`, or throws an error naming `fn`. */
export type LoadAsset = <K extends keyof AssetLoaders>(
    fn: string,
    loader: K,
    path: string,
) => ReturnType<AssetLoaders[K]>;

/** How a param of one schema type goes from what a patch writes to what the engine reads. */
interface AssetParamType<K extends AssetKind & keyof AssetLoaders> {
    /** The kind of file the param names, which is also the loader that reads it. */
    kind: K;
    /** Whether a value is already the engine's value, so it passes through. */
    isResolved: (value: unknown) => boolean;
    /**
     * The engine's value for `value` as written; `rooted` checks and returns a
     * path of this kind, and `load` reads the file at a checked path.
     */
    resolve: (
        fn: string,
        value: unknown,
        rooted: (path: unknown) => string,
        load: (path: string) => ReturnType<AssetLoaders[K]>,
    ) => unknown;
}

/** An asset param type whose resolver checks and loads paths of its own kind. */
interface AssetParam {
    /**
     * The engine's value for `value`, which passes through when already
     * resolved; errors name `fn`.
     */
    resolve: (fn: string, value: unknown, load: LoadAsset) => unknown;
}

function assetParam<K extends AssetKind & keyof AssetLoaders>(
    type: AssetParamType<K>,
): AssetParam {
    return {
        resolve: (fn, value, load) =>
            type.isResolved(value)
                ? value
                : type.resolve(
                      fn,
                      value,
                      (path) => assetPath(fn, path, type.kind),
                      (path) => load(fn, type.kind, path),
                  ),
    };
}

/** The `wav` param value that refers to decoded audio held by the engine. */
export function wavRef(info: LoadedWav) {
    return {
        type: 'wav_ref' as const,
        path: info.path,
        channels: info.channels,
        mtime: info.mtime,
    };
}

/** A `wav` param value already resolved to the engine's reference. */
export function isWavRef(value: unknown): value is ReturnType<typeof wavRef> {
    return (
        typeof value === 'object' &&
        value !== null &&
        'type' in value &&
        value.type === 'wav_ref'
    );
}

/**
 * Every asset param type, by schema type name. A new type is a Rust param
 * type, its name in `ASSET_PARAM_TYPE_NAMES`, and an entry here.
 */
export const ASSET_PARAM_TYPES: Record<AssetParamTypeName, AssetParam> = {
    Wav: assetParam({
        kind: 'wav',
        isResolved: isWavRef,
        // The reference carries the file's lane count, so the file loads
        // while the patch runs.
        resolve: (_fn, value, rooted, load) => wavRef(load(rooted(value))),
    }),
    Dx7Voice: assetParam({
        kind: 'syx',
        isResolved: isDx7VoiceRef,
        resolve: (fn, value, rooted, load) => {
            const { path, voice } =
                typeof value === 'string'
                    ? splitVoice(value)
                    : { path: value, voice: null };
            const checked = rooted(path);
            return selectVoice(fn, checked, voice, load(checked));
        },
    }),
};
