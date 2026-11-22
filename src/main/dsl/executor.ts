import vm from 'node:vm';
import type { ModuleSchema, PatchGraph } from '@modular/core';
import {
    DSLContext,
    hz,
    note,
    setActiveSpanRegistry,
    setDSLWrapperLineOffset,
    captureSourceLocation,
    deriveLaneCountChecked,
    spreadNoteLiterals,
} from './factories';
import { errorSourceLocation, lineSuffix } from './captureSourceLocation';
import type {
    AnyCollection,
    BufferOutputRef,
    GrooveTemplate,
    Signal,
    PolySignal,
    SourceLocation,
    Collection,
    CollectionWithRange,
} from './GraphBuilder';
import {
    $r,
    $cartesian,
    BaseCollection,
    DeferredModuleOutput,
    DeferredCollection,
    Bus,
    Group,
    cycleToLanes,
    replaceSignals,
    PORT_MAX_LANES,
} from './GraphBuilder';
import type { ChainFunction } from './chainModules';
import { $g1, $g2, $g3 } from './signalGroups';
import { createTrackTap, type TrackTap } from './trackTap';
import { analyzeSourceSpans } from './analyzeSource';
import type { CallSiteSpanRegistry } from './analyzeSource';
import type { InterpolationResolutionMap } from '../../shared/dsl/spanTypes';
import { setActiveInterpolationResolutions } from '../../shared/dsl/spanTypes';
import type { SliderDefinition } from '../../shared/dsl/sliderTypes';
import type { ButtonDefinition } from '../../shared/dsl/buttonTypes';
import type { EqPanelDef } from '../../shared/dsl/eqPanelTypes';
import { GATE_HIGH_VOLTAGE } from '../../shared/dsl/buttonTypes';
import { createControls } from './controls';
import { VIDEO_CV_RANGE } from '../../shared/dsl/controlRanges';
import { assertControlsPlaced } from './controlPlacement';
import type { OutputResolutionRecord } from '../../shared/dsl/outputResolutionRule';
import { $, $arrange } from './miniNotation';
import { DELAY_SITE, SUPER_SITE } from './signalSiteTables';
import { dslModules } from '../../shared/dsl/dslModuleDocs';
import { chainNames } from '../../shared/dsl/referenceDocs';
import { outputSetOf } from './outputResolution';
import { superDefaults } from '../../shared/dsl/superDefaults';
import { setActiveCycleFactory } from './patterns';
import { VideoGraphBuilder } from './video/VideoGraphBuilder';
import type { VideoOutput } from './video/VideoOutput';
import type {
    CompiledVideoShader,
    VideoPreviewSite,
} from '../../shared/video/videoGraph';
import { compileVideoGraph } from './video/wgslCompiler';
import { VIDEO_DOCS } from '../../shared/dsl/videoDocs';
import { enoscScalePresets } from '../../shared/dsl/enoscScales';
import { assetPath } from './assetPath';
import {
    prepareTrackLabels,
    type TrackLabelProgram,
} from './trackLabelRuntime';

// Augment Array.prototype with pipe() for TypeScript
declare global {
    interface Array<T> {
        pipe<U>(this: this, pipelineFunc: (self: this) => U): U;
    }
}

/**
 * Result of executing a DSL script.
 */
export interface DSLExecutionResult {
    /** The generated patch graph */
    patch: PatchGraph;
    /** Map from module ID to source location in DSL code */
    sourceLocationMap: Map<string, SourceLocation>;
    /** Interpolation resolution map for template literal const redirects */
    interpolationResolutions: InterpolationResolutionMap;
    /** Slider definitions created by $slider() DSL function calls */
    sliders: SliderDefinition[];
    /** Button definitions created by $btn()/$toggleBtn() DSL function calls */
    buttons: ButtonDefinition[];
    /** Full call expression spans for DSL methods (.scope(), $slider(), etc.) */
    callSiteSpans: CallSiteSpanRegistry;
    /** Inline panels requested by `ui: true` EQs */
    eqPanels: EqPanelDef[];
    /** Shader for the video renderer; null when the patch has no `$v.out` or `$v.preview` */
    video: CompiledVideoShader | null;
    /** One entry per `$v.preview` call, in the order the shader draws them */
    videoPreviews: VideoPreviewSite[];
    /** Every contextual output choice made while the script ran */
    outputResolutions: OutputResolutionRecord[];
}

/** What the engine reports about a decoded WAV or media audio track. */
interface LoadedWav {
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

/** The `wav` parameter value that refers to decoded audio held by the engine. */
function wavRef(info: LoadedWav) {
    return {
        type: 'wav_ref' as const,
        path: info.path,
        channels: info.channels,
        mtime: info.mtime,
    };
}

/** What `$wavMeta` reports about a file. */
function wavMeta(info: LoadedWav) {
    return {
        path: info.path,
        channels: info.channels,
        sampleRate: info.sampleRate,
        frameCount: info.frameCount,
        duration: info.duration,
        bitDepth: info.bitDepth,
        ...(info.pitch != null && { pitch: info.pitch }),
        ...(info.playback != null && { playback: info.playback }),
        ...(info.bpm != null && { bpm: info.bpm }),
        ...(info.beats != null && { beats: info.beats }),
        ...(info.timeSignature != null && {
            timeSignature: {
                num: info.timeSignature.num,
                den: info.timeSignature.den,
            },
        }),
        ...(info.barCount != null && { barCount: info.barCount }),
        loops: info.loops.map((l) => ({
            type: l.loopType as 'forward' | 'pingpong' | 'backward',
            start: l.start,
            end: l.end,
        })),
        cuePoints: info.cuePoints.map((c) => ({
            position: c.position,
            label: c.label,
        })),
    };
}
export interface DSLExecutionOptions {
    sampleRate?: number;
    /**
     * Channel count of the audio input device, which is how wide `$inputMono`
     * is built (and `$input`, in pairs). Read at execution time, so a device
     * swap reaches them on the next run of the patch. Clamped to
     * 1..PORT_MAX_LANES, and defaults to the maximum for callers with no
     * device to report — the inputs never truncate silently, they only narrow
     * when a host says how wide the device is.
     */
    inputChannels?: number;
    /** Whether a workspace-relative path names an existing file, for media errors. */
    mediaExists?: (path: string) => boolean;
    workspaceRoot?: string | null;
    /** Decodes a WAV file named by a `/`-rooted workspace path. */
    loadWav?: (path: string) => LoadedWav;
    /**
     * Decodes the audio track of a workspace media file, or returns null when
     * it has none. The returned `path` names the decoded audio for a `Wav`.
     */
    loadMediaAudio?: (path: string) => LoadedWav | null;
}

// Install pipe() on Array.prototype so arrays in the DSL can use it.
// Non-enumerable to avoid polluting for-in loops.
if (typeof Array.prototype.pipe !== 'function') {
    Object.defineProperty(Array.prototype, 'pipe', {
        configurable: true,
        enumerable: false,
        value: function pipe<T>(
            this: unknown,
            pipelineFunc: (self: typeof this) => T,
        ): T {
            return pipelineFunc(this);
        },
        writable: true,
    });
}

/** The forms in which DSL errors name their own line. */
const DSL_LINE_TEXT = /\bat line \d+|\(line \d+\)/;

/**
 * Execute a DSL script and return the resulting PatchGraph with source locations.
 */
export function executePatchScript(
    source: string,
    schemas: ModuleSchema[],
    options: DSLExecutionOptions = {},
): DSLExecutionResult {
    // Create DSL context
    // Console.log('Executing DSL script with schemas:', schemas);
    const context = new DSLContext(schemas);
    const sampleRate = options.sampleRate ?? 48_000;

    // Create the execution environment with all DSL functions
    // Remove _clock from user-facing namespace (it's internal, used only for ROOT_CLOCK)
    const {
        _clock,
        _videoTap,
        _mediaAudio,
        _smoothControl,
        ...userNamespaceTree
    } = context.namespaceTree;

    if (typeof _clock !== 'function') {
        throw new Error(
            'DSL execution error: "_clock" module not found in schemas',
        );
    }

    const signal = context.namespaceTree['$signal'];
    if (typeof signal !== 'function') {
        throw new Error(
            'DSL execution error: "$signal" module not found in schemas',
        );
    }

    // Create default clock module that runs at 120 BPM
    const $clock = _clock(120, 4, 4, {
        id: 'ROOT_CLOCK',
    });

    const inputChannels = Math.min(
        Math.max(options.inputChannels ?? PORT_MAX_LANES, 1),
        PORT_MAX_LANES,
    );
    const rootInput = signal(
        Array.from({ length: inputChannels }, (_, i) => ({
            lane: i,
            module: 'HIDDEN_AUDIO_IN',
            port: 'input',
            type: 'cable',
        })),
        { id: 'ROOT_INPUT' },
    );

    // `$input` pairs the device channels into stereo lanes, (0, 1), (2, 3), …,
    // with an odd last channel on both sides. It is built on first read, so a
    // patch that never reads it runs no `$stereo`.
    const stereo = context.namespaceTree['$stereo'];
    if (typeof stereo !== 'function') {
        throw new Error(
            'DSL execution error: "$stereo" module not found in schemas',
        );
    }
    let stereoInput: unknown;
    const readStereoInput = () => {
        if (stereoInput === undefined) {
            const channels = [...rootInput];
            const left = channels.filter((_, i) => i % 2 === 0);
            const right = left.map((_, i) => channels[2 * i + 1] ?? left[i]);
            stereoInput = stereo(left, right, { id: 'ROOT_INPUT_STEREO' });
        }
        return stereoInput;
    };

    // Create functions to set global tempo and output gain
    const builder = context.getBuilder();
    const $setTempo = (tempo: number) => {
        if (
            typeof tempo !== 'number' ||
            !Number.isFinite(tempo) ||
            tempo <= 0
        ) {
            throw new Error(
                `$setTempo: tempo must be a finite number greater than 0, got ${tempo}`,
            );
        }
        builder.setTempo(tempo);
    };
    const $setMainGain = (gain: PolySignal) => {
        builder.setMainGain(gain);
    };

    const $setTimeSignature = (numerator: number, denominator: number) => {
        if (!Number.isInteger(numerator) || numerator < 1) {
            throw new Error(
                `$setTimeSignature: numerator must be a positive integer, got ${numerator}`,
            );
        }
        if (!Number.isInteger(denominator) || denominator < 1) {
            throw new Error(
                `$setTimeSignature: denominator must be a positive integer, got ${denominator}`,
            );
        }
        builder.setTimeSignature(numerator, denominator);
    };

    // Shape is checked here; step ordering and ranges are validated in Rust.
    const $setGroove = (template: GrooveTemplate) => {
        if (
            typeof template !== 'object' ||
            template === null ||
            !Array.isArray(template.timing)
        ) {
            throw new Error(
                '$setGroove: expected a groove template with a `timing` array, e.g. $setGroove($swing(0.6))',
            );
        }
        builder.setGroove(template);
    };

    const $swing = (
        ratio: number,
        config: { grid?: number; humanize?: number } = {},
    ): GrooveTemplate => {
        if (typeof ratio !== 'number' || !(ratio > 0 && ratio < 1)) {
            throw new Error(
                `$swing: ratio must be a number between 0 and 1 (0.5 is straight), got ${ratio}`,
            );
        }
        if (
            typeof config !== 'object' ||
            config === null ||
            Array.isArray(config)
        ) {
            throw new Error(
                '$swing: expected a config object as the second argument, e.g. $swing(0.66, { grid: 8 })',
            );
        }
        const { grid = 16, humanize = 0 } = config;
        if (typeof humanize !== 'number' || !(humanize >= 0)) {
            throw new Error(
                `$swing: humanize must be a number 0 or greater, got ${humanize}`,
            );
        }
        // The pair spans two grid steps; the second starts `2 * ratio` steps in.
        const template: GrooveTemplate = { grid, timing: [0, 2 * ratio - 1] };
        if (humanize > 0) {
            template.random = humanize;
        }
        return template;
    };

    /**
     * Create a DeferredCollection with placeholder signals that can be assigned later.
     * Useful for feedback loops and forward references.
     * @param lanes - Number of deferred outputs (1-64, default 1)
     */
    const $deferred = (lanes: number = 1): DeferredCollection => {
        if (lanes < 1 || lanes > PORT_MAX_LANES) {
            throw new Error(
                `deferred() lanes must be between 1 and ${PORT_MAX_LANES}, got ${lanes}`,
            );
        }
        const items: DeferredModuleOutput[] = [];
        for (let i = 0; i < lanes; i++) {
            items.push(new DeferredModuleOutput(builder));
        }
        return new DeferredCollection(...items);
    };

    const $trackTap = (label: string): TrackTap =>
        createTrackTap(builder, label);

    const $bus = (...args: unknown[]): Collection => {
        if (args.length > 0) {
            throw new Error(
                '$bus takes no arguments: it is the mix of what tracks send to it, so chain on it, as in verb: $bus().plate()',
            );
        }
        return new Bus(builder).signal;
    };

    const $group = (...args: unknown[]): Collection => {
        if (args.length > 0) {
            throw new Error(
                '$group takes no arguments: it is the sum of the tracks whose labels name its track, as in kick$drums: … and drums: $group().comp()',
            );
        }
        return new Group(builder).signal;
    };

    const $setEndOfChainCb = (
        cb: (mixed: Collection) => Collection | CollectionWithRange,
    ) => {
        builder.setEndOfChainCb(cb);
    };

    const $buffer = (
        input: Signal | AnyCollection,
        lengthSeconds: number,
        config?: { id?: string },
    ): BufferOutputRef => {
        if (
            typeof lengthSeconds !== 'number' ||
            !Number.isFinite(lengthSeconds)
        ) {
            throw new Error('$buffer() lengthSeconds must be a finite number');
        }
        if (lengthSeconds <= 0) {
            throw new Error(
                `$buffer() lengthSeconds must be greater than 0, got ${lengthSeconds}`,
            );
        }

        const sourceLocation = captureSourceLocation();

        return builder.buildModule(
            '$buffer',
            config?.id,
            sourceLocation,
            (node): BufferOutputRef => {
                // Resolve the input signal and set params
                const notes = spreadNoteLiterals(input);
                node._setParam('input', replaceSignals(notes));
                builder.setModuleChannels(
                    node.id,
                    builder.liftedChannelsOf([notes]),
                );
                node._setParam('length', lengthSeconds);

                // Derive lane count from the input signal
                const derivedLanes = deriveLaneCountChecked(
                    '$buffer',
                    node.getParamsSnapshot(),
                    sourceLocation,
                );
                if (derivedLanes !== undefined) {
                    node._setDerivedLaneCount(derivedLanes);
                }
                const lanes = derivedLanes ?? 1;

                const frameCount = Math.max(
                    1,
                    Math.ceil(lengthSeconds * sampleRate),
                );

                return {
                    type: 'buffer_ref',
                    module: node.id,
                    port: 'buffer',
                    lanes,
                    frameCount,
                };
            },
        );
    };

    const $cycle = userNamespaceTree['$cycle'];
    if (typeof $cycle !== 'function') {
        throw new Error(
            'DSL execution error: "$cycle" module not found in schemas',
        );
    }
    // Numbers convert here; a frequency that includes a signal converts on the
    // audio thread through `_hz`. Note and Hz strings are already pitches, so
    // they are rejected rather than read as a frequency.
    const $hz = (frequency: number | PolySignal) => {
        if (typeof frequency === 'number') {
            return hz(frequency);
        }
        const items = Array.isArray(frequency) ? frequency : [frequency];
        for (const item of items) {
            if (typeof item === 'string') {
                throw new Error(
                    `$hz takes a frequency in Hz, not "${item}": note and Hz strings are already pitches`,
                );
            }
            if (typeof item === 'number') {
                hz(item);
            }
        }
        if (items.every((item) => typeof item === 'number')) {
            return items.map((item) => hz(item));
        }
        return builder.getFactory('_hz')(frequency) as Collection;
    };
    const $mix = userNamespaceTree['$mix'];
    const $delayRead = userNamespaceTree['$delayRead'];
    const $clamp = userNamespaceTree['$clamp'];
    if (
        typeof $mix !== 'function' ||
        typeof $delayRead !== 'function' ||
        typeof $clamp !== 'function'
    ) {
        throw new Error(
            'DSL execution error: "$delay" requires "$mix", "$delayRead", and "$clamp" modules',
        );
    }

    // Documented with `$delay`'s declaration (`dslModuleDocs.ts`).
    interface DelayOptions {
        feedback?: PolySignal;
        feedbackCb?: (mixed: Collection) => Collection;
        maxTime?: number;
    }

    // Lane count of a polysignal argument: collections and arrays
    // contribute their (flattened) length; a single output or scalar counts as
    // one. Used to size the feedback `$deferred` to the widest param.
    const polyLanes = (v: unknown): number => {
        if (v instanceof BaseCollection) return Math.max(1, v.length);
        if (Array.isArray(v)) {
            return v.reduce((n, e) => n + polyLanes(e), 0);
        }
        return 1;
    };

    // Sums `input` with the attenuated feedback, passes the mix through
    // `feedbackCb`, writes it into a `$buffer` and reads it back `time` late.
    const $delay = (
        input: Collection,
        time: PolySignal,
        opts?: DelayOptions,
    ): Collection => {
        const signals: Record<string, unknown> = {
            feedback: opts?.feedback,
            input,
            time,
        };
        // The modules are built out of sight, so the module roots among the
        // signals are recorded against the `$delay` call itself.
        return builder.outputResolver.withCall(
            Object.entries(DELAY_SITE.inputs).map(([name, inputClass]) => [
                name,
                signals[name],
                inputClass,
            ]),
            () => {
                const feedback = $clamp(opts?.feedback ?? 2.5, {
                    min: 0,
                    max: 5,
                }) as Collection;
                const maxTime = opts?.maxTime ?? 5;
                // Size the feedback path to the widest polysignal param so a poly
                // `time` or `feedback` is not truncated when it exceeds `input`.
                const lanes = Math.max(
                    1,
                    polyLanes(input),
                    polyLanes(time),
                    polyLanes(opts?.feedback ?? 2.5),
                );
                const def = $deferred(lanes);
                // Broadcast a narrower input across the feedback width so every lane
                // is excited by the dry signal (otherwise the extra lanes carry
                // only feedback and decay to silence — `$mix` skips, not cycles, a
                // narrower input group's missing lanes).
                const mixed = $mix([
                    cycleToLanes(input, lanes),
                    def,
                ]) as Collection;
                // Colour every recirculation: process the dry-plus-feedback mix before
                // it is written to the buffer.
                const { feedbackCb } = opts ?? {};
                const buffered = feedbackCb
                    ? builder.outputResolver.withUserCode(() =>
                          feedbackCb(mixed),
                      )
                    : mixed;
                const buf = $buffer(buffered, maxTime);
                const delayed = $delayRead(buf, time) as Collection;
                def.set(builder.getFactory('$amp')(delayed, feedback));
                return delayed;
            },
        );
    };

    const $unison = userNamespaceTree['$unison'];
    const $stereoFold = userNamespaceTree['$stereoFold'];
    const superDefault = superDefaults(schemas);
    if (
        typeof $unison !== 'function' ||
        typeof $stereoFold !== 'function' ||
        !superDefault
    ) {
        throw new Error(
            'DSL execution error: "$super" requires "$unison", "$stereoFold" and "$supersaw"',
        );
    }

    // Documented with `$super`'s declaration (`dslModuleDocs.ts`).
    interface SuperOptions {
        unison?: number;
        detune?: PolySignal;
        width?: PolySignal;
    }

    type SuperVoice = (
        voices: Collection,
        options: { phaseOffset: Collection },
    ) => Collection;

    const $super = (
        freq: PolySignal,
        cb: SuperVoice,
        opts: SuperOptions = {},
    ): Collection => {
        if (typeof cb !== 'function') {
            throw new TypeError(
                "$super: pass a function that builds one voice, e.g. $super('c3', (freq, { phaseOffset }) => $saw(freq, { phaseOffset }))",
            );
        }
        if (typeof opts !== 'object' || opts === null) {
            throw new TypeError(
                '$super: options must be an object, e.g. { unison: 7, detune: 0.3, width: 4 }',
            );
        }
        const { unison = superDefault.unison, detune, width } = opts;
        const signals: Record<string, unknown> = { detune, freq, width };
        return builder.outputResolver.withCall(
            Object.entries(SUPER_SITE.inputs).map(([name, inputClass]) => [
                name,
                signals[name],
                inputClass,
            ]),
            () => {
                const voices = $unison(freq, unison, detune) as Collection;
                const phaseOffset = outputSetOf(voices)?.values
                    .phase as Collection;
                const voice = builder.outputResolver.withUserCode(() =>
                    cb(voices, { phaseOffset }),
                );
                return $stereoFold(voice, unison, width) as Collection;
            },
        );
    };

    // Modules built from others have no schema, so the ones their docs mark
    // `chainable` (`dslModuleDocs.ts`) are made chain methods here.
    const builtChains: Record<string, ChainFunction> = {
        delay: (self, ...args) =>
            $delay(
                self as Collection,
                ...(args as [PolySignal, DelayOptions?]),
            ),
        super: (self, ...args) =>
            $super(
                self as Collection,
                ...(args as [SuperVoice, SuperOptions?]),
            ),
    };
    const chained = chainNames(dslModules(schemas));
    const unmatched = [
        ...chained.filter((name) => !(name in builtChains)),
        ...Object.keys(builtChains).filter((name) => !chained.includes(name)),
    ];
    if (unmatched.length > 0) {
        throw new Error(
            `DSL execution error: chainable in the docs but not here, or here but not in the docs: ${unmatched.join(', ')}`,
        );
    }
    for (const name of chained) {
        builder.registerChainMethod(name, builtChains[name]);
    }

    const $keyframes = userNamespaceTree['$keyframes'];
    if (typeof $keyframes !== 'function') {
        throw new Error(
            'DSL execution error: "$cross" requires "$keyframes" module',
        );
    }

    const $cross = (
        count: number,
        playhead: PolySignal,
        range: [number, number] = [0, 5],
        interpolationType?: string,
    ): Collection => {
        const frames: [Signal[], number][] = [];
        for (let i = 0; i < count; i++) {
            const frame: Signal[] = Array.from(
                { length: count },
                () => range[0],
            );
            frame[i] = range[1];
            frames.push([frame, count > 1 ? i / (count - 1) : 0]);
        }
        return $keyframes(frames, {
            playhead,
            interpolationType,
        }) as Collection;
    };

    if (typeof _smoothControl !== 'function') {
        throw new Error(
            'DSL execution error: "_smoothControl" module not found in schemas',
        );
    }

    // Control factories; their collectors fill as the patch calls them.
    const controlFactories = { $signal: signal, _smoothControl };
    const controls = createControls({
        rangedSignal: (moduleType, value, id, range) =>
            controlFactories[moduleType](value, {
                ...range,
                id,
            }) as CollectionWithRange,
    });

    if (typeof _videoTap !== 'function') {
        throw new Error(
            'DSL execution error: "_videoTap" module not found in schemas',
        );
    }
    if (typeof _mediaAudio !== 'function') {
        throw new Error(
            'DSL execution error: "_mediaAudio" module not found in schemas',
        );
    }
    const videoBuilder = new VideoGraphBuilder({
        controlValue: (moduleId) => {
            const slider = controls.sliders.find(
                (s) => s.moduleId === moduleId,
            );
            if (slider) return slider.value;
            const button = controls.buttons.find(
                (b) => b.moduleId === moduleId,
            );
            if (button) return button.value ? GATE_HIGH_VOLTAGE : 0;
            return undefined;
        },
        publishTap: (output, slot) => {
            _videoTap(output, slot);
        },
        cvSignal: (id) =>
            signal(0, { ...VIDEO_CV_RANGE, id }) as CollectionWithRange,
        mediaAudio: (path, playback) => {
            if (!options.loadMediaAudio) {
                throw new Error(
                    '$v.video(...).audio: media audio loading is not available',
                );
            }
            const info = timedWavLoad(() => options.loadMediaAudio!(path));
            if (info === null) {
                throw new Error(`$v.video(...).audio: "/${path}" has no audio`);
            }
            return _mediaAudio(wavRef(info), playback) as Collection;
        },
        mediaExists: options.mediaExists,
        sourceLocation: captureSourceLocation,
        constantSignal: (value) => signal(value) as Collection,
    });

    /** `$v.scopeXY(...)` shown as the patch's picture. */
    const $scopeXY = (
        ...args: Parameters<VideoGraphBuilder['scopeXY']>
    ): VideoOutput => videoBuilder.out(videoBuilder.scopeXY(...args));

    // Wall-clock time this run spends inside synchronous loadWav host calls.
    // The vm timeout below measures total wall-clock time of the run, so this
    // distinguishes a patch stuck in a loop from one legitimately spending
    // its budget decoding samples from disk. `wavLoadStart` marks a load in
    // flight: the timeout terminates all JS in the isolate, so the
    // post-load accumulation may never run and the catch block below must
    // count the interrupted load itself.
    let wavLoadMs = 0;
    let wavLoadStart: number | null = null;

    /** Runs a synchronous host load, billing its time to WAV loading. */
    const timedWavLoad = <T>(load: () => T): T => {
        const loadStart = performance.now();
        wavLoadStart = loadStart;
        try {
            return load();
        } finally {
            // Also on throw: the time was still spent loading, and a
            // stale wavLoadStart would bill everything after a caught
            // load failure — even an infinite loop — to WAV loading.
            wavLoadMs += performance.now() - loadStart;
            wavLoadStart = null;
        }
    };

    /** Decodes the file at a `/`-rooted workspace path, or an error naming `fn`. */
    const loadWavFile = (fn: string, path: unknown): LoadedWav => {
        const rooted = assetPath(fn, path, 'wav');
        if (!options.loadWav) {
            throw new Error(`${fn}: WAV loading is not available`);
        }
        return timedWavLoad(() => options.loadWav!(rooted));
    };
    context.setWavResolver((module, path) => wavRef(loadWavFile(module, path)));

    /** The format, tempo, loops and cue points of a WAV file. */
    const $wavMeta = (path: string) => wavMeta(loadWavFile('$wavMeta', path));

    /**
     * $phase.* DSL helpers produce phase-warp descriptors for modules that
     * accept a `PhaseWarp` (`$wavetable`, `$enosc`, `$pWarp`).
     *
     * Each helper returns a plain JSON object whose shape matches the Rust
     * `PhaseWarp` deserializer (`#[serde(tag = "type", rename_all = "camelCase")]`).
     *
     * Inner signal-valued fields are passed through `replaceSignals` so that
     * ModuleOutputs / Collections are converted to the same wire format used
     * for module-factory params.
     *
     * Warps are composable: each returned descriptor has a `.pipe(fn)` method,
     * and the optional second argument to each helper feeds this warp's output
     * phase into `next`.
     */
    function wrapWarp(descriptor: Record<string, unknown>): Record<
        string,
        unknown
    > & {
        pipe: <T>(fn: (self: Record<string, unknown>) => T) => T;
    } {
        const t = { ...descriptor } as Record<string, unknown> & {
            pipe: <T>(fn: (self: Record<string, unknown>) => T) => T;
        };
        Object.defineProperty(t, 'pipe', {
            value: <T>(fn: (self: typeof t) => T): T => fn(t),
            enumerable: false,
            writable: false,
            configurable: false,
        });
        return t;
    }

    const warpHelper =
        (type: string, field: string) => (value: unknown, next?: unknown) => {
            const t = wrapWarp({
                type,
                [field]: replaceSignals(spreadNoteLiterals(value)),
            });
            return next !== undefined
                ? wrapWarp({ type: 'pipe', first: t, second: next })
                : t;
        };

    const $phase = {
        mirror: warpHelper('mirror', 'amount'),
        bend: warpHelper('bend', 'amount'),
        sync: warpHelper('sync', 'ratio'),
        fold: warpHelper('fold', 'amount'),
        pwm: warpHelper('pwm', 'width'),
        pulsar: warpHelper('pulsar', 'amount'),
        crush: warpHelper('crush', 'amount'),
        feedback: warpHelper('feedback', 'amount'),
    };

    /**
     * `$enosc.scale.*` holds the `EnoscScale` descriptors `$enosc`'s `scale`
     * param takes: each preset is `{ type: <preset> }`, and `custom` returns
     * `{ type: 'custom', notes, repeatsOctave }`. The engine validates them.
     */
    // Each preset is a getter, so every use gets its own descriptor object.
    const enoscScale: Record<string, unknown> = {};
    for (const { name: type } of enoscScalePresets(schemas)) {
        Object.defineProperty(enoscScale, type, {
            get: () => ({ type }),
            enumerable: true,
        });
    }
    enoscScale.custom = (notes: string, repeatsOctave: unknown) => {
        if (typeof notes !== 'string') {
            throw new TypeError(
                '$enosc.scale.custom: notes must be a space-separated string of semitones, e.g. "0 3 7"',
            );
        }
        const values = notes.trim().split(/\s+/).filter(Boolean).map(Number);
        if (values.some((v) => !Number.isFinite(v))) {
            throw new TypeError(
                `$enosc.scale.custom: "${notes}" is not a space-separated list of numbers`,
            );
        }
        if (typeof repeatsOctave !== 'boolean') {
            throw new TypeError(
                '$enosc.scale.custom: repeatsOctave must be true or false',
            );
        }
        return { type: 'custom', notes: values, repeatsOctave };
    };
    const enoscFactory = userNamespaceTree['$enosc'];
    if (typeof enoscFactory === 'function') {
        Object.defineProperty(enoscFactory, 'scale', {
            value: Object.freeze(enoscScale),
            enumerable: true,
            configurable: true,
        });
    }

    const dslGlobals = {
        // Prefixed namespace tree (modules and namespaces, minus _clock)
        ...userNamespaceTree,
        // Helper functions with $ prefix
        $hz,
        $note: note,
        // Mini-notation patterns: $(source), $`source` and $arrange(...) of
        // sections. Each is the $cycle that plays it.
        $,
        $arrange,
        // Phase-warp descriptors for $wavetable, $enosc and $pWarp
        $phase,
        // Collection helpers
        $c: builder.$c.bind(builder),
        $r,
        $cartesian,
        // Cartesian signal groups — tag a param value so groups multiply
        // across a module instead of cycling together
        $g1,
        $g2,
        $g3,
        // Deferred signal helper
        $deferred,
        // Panel controls
        $slider: controls.$slider,
        $btn: controls.$btn,
        $toggleBtn: controls.$toggleBtn,
        $cGroup: controls.$cGroup,
        // Bus
        $bus,
        $group,
        $trackTap,
        // Global settings
        $setTempo,
        $setMainGain,
        $setTimeSignature,
        $setGroove,
        $swing,
        $setEndOfChainCb,
        // XY background oscilloscope
        $scopeXY,
        // Video synthesis
        $v: Object.fromEntries(
            VIDEO_DOCS.map(({ name }) => [
                name,
                videoBuilder[name as keyof VideoGraphBuilder],
            ]),
        ),
        $buffer,
        $delay,
        $super,
        $cross,
        $wavMeta,
        // Built-in modules
        $clock,
        $inputMono: rootInput,
    };

    // Console.log(dslGlobals);

    // Build the script body - count wrapper lines for source mapping.
    // Stack-trace line numbers are relative to the compiled vm script: the
    // template literal's leading newline, the 'use strict' line, and the
    // IIFE opener put user code at line 4 in stack traces.
    const wrapperLineCount = 3;
    setDSLWrapperLineOffset(wrapperLineCount);

    // Analyze source code to extract argument spans before execution
    // The registry maps call-site keys (line:column) to argument span info
    const {
        registry: spanRegistry,
        interpolationResolutions,
        callSiteSpans,
    } = analyzeSourceSpans(source, schemas);
    setActiveSpanRegistry(spanRegistry);
    setActiveInterpolationResolutions(interpolationResolutions);
    setActiveCycleFactory($cycle);

    let trackLabels: TrackLabelProgram;
    try {
        trackLabels = prepareTrackLabels(source);
    } catch (error) {
        throw new Error(`DSL execution error: ${(error as Error).message}`, {
            cause: error,
        });
    }

    // The user source runs as a function body (an IIFE), not at the script's
    // top level, so top-level `return` is legal in a patch script.
    const scriptBody = `
    'use strict';
    (function () {
    ${trackLabels.runSource}
    })();
  `;

    // Cap on synchronous script execution. A non-terminating patch script
    // (e.g. an accidental infinite loop) raises a catchable error instead of
    // blocking the Electron main process forever.
    const executionTimeoutMs = 5000;

    try {
        // Execute the script in a vm context so the timeout applies. The DSL
        // globals are the sandbox's globals; they are host objects, so
        // instanceof checks against host classes keep working. The default
        // script filename contains `<anonymous>`, which captureSourceLocation
        // relies on to find DSL stack frames.
        const script = new vm.Script(scriptBody);
        const sandbox = vm.createContext({
            ...dslGlobals,
            console,
            structuredClone,
        });
        Object.defineProperty(sandbox, '$input', {
            enumerable: true,
            get: readStereoInput,
        });
        trackLabels.install(sandbox, builder);
        // The sandbox is its own realm: array literals in the patch script use
        // the sandbox's Array.prototype, so pipe() must be installed there too
        // (mirroring the host-side installation above). The explicit filename
        // keeps `<anonymous>` out of the installer's stack frames, so
        // captureSourceLocation never mistakes the installer's `pipe` frame
        // for the user's call site.
        vm.runInContext(
            `Object.defineProperty(Array.prototype, 'pipe', {
                configurable: true,
                enumerable: false,
                value: function pipe(pipelineFunc) { return pipelineFunc(this); },
                writable: true,
            });`,
            sandbox,
            { filename: 'dsl-pipe-installer.js' },
        );
        script.runInContext(sandbox, { timeout: executionTimeoutMs });
        assertControlsPlaced(source, [
            ...controls.sliders,
            ...controls.buttons,
            ...controls.groups,
        ]);

        // The video graph goes first: building it publishes the audio taps
        // it reads into the patch.
        const videoGraph = videoBuilder.build();
        const resultBuilder = context.getBuilder();
        const patch = resultBuilder.toPatch();
        const sourceLocationMap = resultBuilder.getSourceLocationMap();

        return {
            buttons: controls.buttons,
            callSiteSpans,
            eqPanels: resultBuilder.getEqPanels(),
            interpolationResolutions,
            outputResolutions: resultBuilder.outputResolver.records,
            patch,
            sliders: controls.sliders,
            sourceLocationMap,
            video: videoGraph === null ? null : compileVideoGraph(videoGraph),
            videoPreviews: videoBuilder.getPreviewSites(),
        };
    } catch (error) {
        if (
            (error as NodeJS.ErrnoException | null)?.code ===
            'ERR_SCRIPT_EXECUTION_TIMEOUT'
        ) {
            // The timeout counts wall-clock time, including synchronous host
            // call-outs. When most of the budget went to loading WAV files
            // (slow/cold disk, big library), say so — the wav cache keeps
            // everything already decoded, so a re-run makes progress.
            const seconds = executionTimeoutMs / 1000;
            const totalWavMs =
                wavLoadMs +
                (wavLoadStart !== null ? performance.now() - wavLoadStart : 0);
            const message =
                totalWavMs >= executionTimeoutMs / 2
                    ? `DSL execution error: Patch script timed out after ${seconds}s, ${(totalWavMs / 1000).toFixed(1)}s of it loading WAV files — loaded files stay cached, so run again to continue`
                    : `DSL execution error: Patch script timed out after ${seconds}s — check for infinite loops`;
            throw new Error(message, { cause: error });
        }
        // Errors thrown by code inside the vm are sandbox-realm Error
        // instances, so duck-type on `message` rather than instanceof.
        const message =
            typeof error === 'object' &&
            error !== null &&
            typeof (error as { message?: unknown }).message === 'string'
                ? (error as { message: string }).message
                : null;
        if (message !== null) {
            // A position past the patch's last line is the wrapper's closing
            // line, where V8 reports input that ends inside an open bracket.
            const location = errorSourceLocation(error);
            const inSource =
                location && location.line <= source.split('\n').length
                    ? location
                    : undefined;
            const located = DSL_LINE_TEXT.test(message)
                ? message
                : message + lineSuffix(inSource);
            throw new Error(`DSL execution error: ${located}`, {
                cause: error,
            });
        }
        throw error;
    } finally {
        // The spans are baked into module state via ARGUMENT_SPANS_KEY, and
        // patterns play only while the script runs.
        setActiveSpanRegistry(null);
        setActiveCycleFactory(null);
        // NOTE: Do NOT clear interpolation resolutions here. They are read
        // Asynchronously by moduleStateTracking during decoration polling and
        // Must persist until the next execution replaces them.
    }
}
