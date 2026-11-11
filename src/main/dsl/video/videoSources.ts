import type { Collection } from '../GraphBuilder';
import type { VideoSourceDef } from '../../../shared/video/videoGraph';
import {
    describe,
    type VideoCameraConfig,
    type VideoCore,
    type VideoMediaConfig,
    type VideoScreenConfig,
    type VideoVideoConfig,
} from './videoBuilderTypes';
import type { VideoOutput } from './VideoOutput';
import { assetPath } from '../assetPath';

const FITS = ['cover', 'contain', 'stretch'];
/** The fastest playback rate the browser engine's media element supports. */
const MAX_SPEED = 16;

/**
 * The workspace-relative path of the file a `/`-rooted asset path names, or
 * an error naming `fn` when it is not a file of `kind` in the workspace folder.
 */
function mediaPath(
    core: VideoCore,
    fn: string,
    path: unknown,
    kind: 'image' | 'video',
): string {
    const rooted = assetPath(fn, path, kind);
    const relative = rooted.slice(1);
    if (!core.mediaExists(relative)) {
        throw new Error(`${fn}: no file "${rooted}" in the workspace folder`);
    }
    return relative;
}

/** The `speed` and `loop` options of `$v.video` as the fields of a source definition. */
function playback(
    config: VideoVideoConfig | undefined,
): Pick<VideoSourceDef, 'speed' | 'loopStart' | 'loopEnd'> {
    const out: Pick<VideoSourceDef, 'speed' | 'loopStart' | 'loopEnd'> = {};
    const speed = config?.speed ?? 1;
    if (typeof speed !== 'number' || !(speed >= 0 && speed <= MAX_SPEED)) {
        throw new Error(
            `$v.video: speed must be a number from 0 to ${MAX_SPEED} (video cannot play backwards), got ${describe(speed)}`,
        );
    }
    if (speed !== 1) out.speed = speed;
    const loop = config?.loop;
    if (loop !== undefined) {
        const [start, end] = Array.isArray(loop) ? loop : [];
        const valid =
            Array.isArray(loop) &&
            loop.length >= 1 &&
            loop.length <= 2 &&
            typeof start === 'number' &&
            Number.isFinite(start) &&
            start >= 0 &&
            (end === undefined ||
                (typeof end === 'number' &&
                    Number.isFinite(end) &&
                    end > start));
        if (!valid) {
            throw new Error(
                `$v.video: loop must be [start] or [start, end] in seconds with 0 <= start < end, got ${describe(loop)}`,
            );
        }
        if (start > 0) out.loopStart = start;
        if (end !== undefined) out.loopEnd = end;
    }
    return out;
}

/** How a source that does not match the frame is fitted, or an error naming `fn`. */
function fitOf(fn: string, config: VideoMediaConfig | undefined): string {
    const fit = config?.fit ?? 'cover';
    if (!FITS.includes(fit)) {
        throw new Error(
            `${fn}: fit must be one of ${FITS.join(', ')}, got "${fit}"`,
        );
    }
    return fit;
}

/**
 * Gives a video's color an `audio` property: the audio track as an audio
 * signal, playing at the video's speed between its loop points. One signal is
 * made however often the property is read.
 */
function withAudio(
    core: VideoCore,
    output: VideoOutput,
    path: string,
    timing: Pick<VideoSourceDef, 'speed' | 'loopStart' | 'loopEnd'>,
): VideoOutput {
    let audio: Collection | undefined;
    Object.defineProperty(output, 'audio', {
        enumerable: true,
        get: () =>
            (audio ??= core.mediaAudio(path, {
                speed: timing.speed ?? 1,
                loopStart: timing.loopStart ?? 0,
                ...(timing.loopEnd !== undefined && {
                    loopEnd: timing.loopEnd,
                }),
            })),
    });
    return output;
}

/** The `$v` functions that read pictures, recordings, cameras and screens. */
export function sourceMethods(core: VideoCore) {
    const source = (def: VideoSourceDef, fit: string): VideoOutput =>
        core.addNode(
            'source',
            {},
            { params: { fit }, source: core.sourceIndex(def) },
        );
    const make =
        (fn: string, kind: 'image' | 'video') =>
        (path: string, config?: VideoVideoConfig): VideoOutput => {
            const fit = fitOf(fn, config);
            if (
                kind === 'image' &&
                (config?.speed !== undefined || config?.loop !== undefined)
            ) {
                throw new Error(`${fn}: speed and loop apply only to video`);
            }
            const normalized = mediaPath(core, fn, path, kind);
            const timing = kind === 'video' ? playback(config) : {};
            const output = source({ kind, path: normalized, ...timing }, fit);
            return kind === 'video'
                ? withAudio(core, output, normalized, timing)
                : output;
        };
    return {
        /**
         * A picture from the workspace folder, drawn to fill the frame as `fit`
         * says.
         */
        image: make('$v.image', 'image'),

        /**
         * A recording from the workspace folder, played in a loop, muted, at
         * `speed` and between the `loop` points.
         */
        video: make('$v.video', 'video'),

        /** The live picture of a camera, as a color. */
        camera: (config?: VideoCameraConfig): VideoOutput => {
            const fit = fitOf('$v.camera', config);
            const { device } = config ?? {};
            if (
                device !== undefined &&
                (typeof device !== 'string' || device.trim() === '')
            ) {
                throw new Error(
                    `$v.camera: device must be part of a camera's name, got ${describe(device)}`,
                );
            }
            return source(
                { kind: 'camera', path: '', ...(device ? { device } : {}) },
                fit,
            );
        },

        /** The live picture of a display, as a color. */
        screen: (config?: VideoScreenConfig): VideoOutput => {
            const fit = fitOf('$v.screen', config);
            const { display } = config ?? {};
            if (
                display !== undefined &&
                (!Number.isInteger(display) || display < 1)
            ) {
                throw new Error(
                    `$v.screen: display must be a whole number from 1, got ${describe(display)}`,
                );
            }
            return source(
                {
                    kind: 'screen',
                    path: '',
                    ...(display !== undefined && display !== 1
                        ? { display }
                        : {}),
                },
                fit,
            );
        },
    };
}
