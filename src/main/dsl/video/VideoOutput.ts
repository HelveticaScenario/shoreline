import type { VideoValue } from '../../../shared/video/videoGraph';

/** The graph builder's `$v` functions, which chain methods forward to. */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export type VideoOps = Record<string, (...args: any[]) => any>;

type ChainCall = (
    ops: VideoOps,
    self: VideoOutput,
    ...args: unknown[]
) => unknown;

/** The signal each chained signal was made from, which `.wet()` fades against. */
const DRY = new WeakMap<object, VideoOutput>();

/**
 * A video signal: an RGBA color at every pixel, from a node, a constant, or
 * the time.
 *
 * It chains the way an audio signal does: the `$v` functions that take a
 * signal first are its methods, with the signal supplied for you, so
 * `x.rotate(0.1)` is `$v.warp(x, { rotate: 0.1 })`. `.wet(mix)` after one
 * crossfades the signal it was called on against the result (0 is the signal,
 * 1 the result). `.pipe` and `.pipeMix` apply a function of your own. `out`,
 * `preview`, `toCV` and `write` end a chain.
 */
export class VideoOutput {
    constructor(
        readonly value: VideoValue,
        /** The builder that made this signal; chain methods call its functions. */
        readonly ops: VideoOps,
    ) {}

    /** The red channel as a gray signal. */
    get r(): unknown {
        return this.ops.channel(this, 'r');
    }

    /** The green channel as a gray signal. */
    get g(): unknown {
        return this.ops.channel(this, 'g');
    }

    /** The blue channel as a gray signal. */
    get b(): unknown {
        return this.ops.channel(this, 'b');
    }

    /** The alpha as a gray signal. */
    get a(): unknown {
        return this.ops.channel(this, 'a');
    }

    /**
     * Crossfades the signal this one was chained from against it: 0 is that
     * signal, 1 this one.
     */
    wet(mix: unknown = 0.5): unknown {
        const dry = DRY.get(this);
        if (dry === undefined) {
            throw new Error(
                'wet: follows a chained function, as in x.kaleid(6).wet(0.5)',
            );
        }
        return this.ops.mix(dry, this, mix);
    }

    /**
     * Calls `fn` with this signal. With an array, calls it once per element,
     * as `fn(this, element)`, and returns the results as an array.
     */
    pipe(fn: unknown, array?: unknown[]): unknown {
        if (typeof fn !== 'function') {
            throw new Error('pipe: expects a function');
        }
        if (array === undefined) return fn(this);
        if (!Array.isArray(array)) {
            throw new Error('pipe: the second argument must be an array');
        }
        return array.map((item) => fn(this, item));
    }

    /** Crossfades this signal against `fn(this)`: 0 is this signal, 1 the result. */
    pipeMix(fn: unknown, mix: unknown = 0.5): unknown {
        if (typeof fn !== 'function') {
            throw new Error('pipeMix: expects a function');
        }
        return this.ops.mix(this, fn(this), mix);
    }
}

/** `$v` functions that are methods of a signal under their own names, with the signal first. */
const FORWARDED = [
    'add',
    'blend',
    'bloom',
    'blur',
    'brightness',
    'channel',
    'color',
    'colorama',
    'comparator',
    'contrast',
    'diff',
    'displace',
    'edges',
    'fold',
    'frameDelay',
    'grain',
    'hsv',
    'hue',
    'invert',
    'kaleid',
    'key',
    'layer',
    'luma',
    'mask',
    'max',
    'min',
    'mix',
    'modulate',
    'modulateHue',
    'modulateKaleid',
    'modulatePixelate',
    'modulateRepeat',
    'modulateRepeatX',
    'modulateRepeatY',
    'modulateRotate',
    'modulateScale',
    'modulateScrollX',
    'modulateScrollY',
    'mult',
    'pixelate',
    'posterize',
    'procAmp',
    'repeat',
    'saturate',
    'scanlines',
    'shift',
    'swiz',
    'thresh',
    'vignette',
    'warp',
    'wrap',
];

const forward = (names: string[]): Record<string, ChainCall> =>
    Object.fromEntries(
        names.map((name) => [
            name,
            (ops: VideoOps, self: VideoOutput, ...args: unknown[]) =>
                ops[name](self, ...args),
        ]),
    );

/** Chain methods: each returns a new signal from this one. */
const PROCESSING: Record<string, ChainCall> = {
    ...forward(FORWARDED),
    osc: (o, s, freq, phase, offset, config) =>
        o.osc(freq, phase, offset, { ...(config as object), input: s }),
    rotate: (o, s, turns) => o.warp(s, { rotate: turns }),
    scale: (o, s, zoom) => o.warp(s, { zoom }),
    scroll: (o, s, x = 0, y = 0) => o.warp(s, { shiftX: x, shiftY: y }),
    tint: (o, s, hue, saturation) => o.hsv(hue ?? 0, saturation ?? 1, s),
};

/** Direct methods that end a chain or tap it: they do not make a new signal to chain on. */
const DIRECT: Record<string, ChainCall> = {
    ...forward(['range', 'out', 'preview', 'toCV']),
    write: (_o, s, buffer) =>
        (buffer as { write(color: VideoOutput): unknown }).write(s),
};

for (const [name, call] of Object.entries(DIRECT)) {
    Object.defineProperty(VideoOutput.prototype, name, {
        configurable: true,
        value(this: VideoOutput, ...args: unknown[]) {
            return call(this.ops, this, ...args);
        },
    });
}

for (const [name, call] of Object.entries(PROCESSING)) {
    Object.defineProperty(VideoOutput.prototype, name, {
        configurable: true,
        value(this: VideoOutput, ...args: unknown[]) {
            const result = call(this.ops, this, ...args);
            if (typeof result === 'object' && result !== null) {
                DRY.set(result, this);
            }
            return result;
        },
    });
}

/** The names of the chain methods, which `.wet()` can follow. */
export const VIDEO_CHAIN_METHODS: readonly string[] = Object.keys(PROCESSING);

/** The names of the methods that end, tap or fade a chain. */
export const VIDEO_DIRECT_METHODS: readonly string[] = [
    ...Object.keys(DIRECT),
    'wet',
];
