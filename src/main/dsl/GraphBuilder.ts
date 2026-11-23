import type {
    BoundsKind,
    ModuleSchema,
    OutputSchema,
    ModuleSpec,
    PatchGraph,
    Scope,
    ScopeLane,
    ScopeMode,
    ScopeSplit,
} from '@modular/core';
import {
    type ControlRange,
    GATE_RANGE,
    OUT_GAIN_RANGE,
    OUT_PAN_RANGE,
    SEND_LEVEL_RANGE,
} from '../../shared/dsl/controlRanges';
import type {
    ControlTap,
    VuControl,
    VuMeterDef,
    VuSendDef,
} from '../../shared/dsl/vuMeterTypes';
import {
    EQ_MODULE_TYPE,
    EQ_SPECTRUM_MS_PER_FRAME,
    type EqPanelDef,
} from '../../shared/dsl/eqPanelTypes';
import {
    DEFAULT_OUTPUT_GAIN,
    UNITY_OUT_GAIN,
} from '../../shared/dsl/vuMeterTypes';
import type { ProcessedModuleSchema } from './paramsSchema';
import { processSchemas } from './paramsSchema';
import {
    carryDrySource,
    createChainInstaller,
    drySourceOf,
    type ChainFunction,
    type ChainHost,
} from './chainModules';
import { captureSourceLocation, lineSuffix } from './captureSourceLocation';
import { type FeedbackTrack, findBusFeedback } from './busFeedback';
import { MIX_SLOT } from '../../shared/dsl/outputResolutionRule';
import {
    markRoot,
    OutputResolver,
    type OutputSet,
    outputSetOf,
} from './outputResolution';
import { isPatternPayload, patternSignal } from './patterns';
import { signalRootOf, carrySignalRoot, setSignalRoot } from './signalRoots';
import { isPlainObject } from './plainObject';
import { trackGates } from '../../shared/dsl/trackGates';
import { trackNameKey } from '../../shared/dsl/trackLabels';
import {
    assertSignalGroupsTopLevelOnly,
    expandSignalGroups,
    isSignalGroupLike,
} from './signalGroups';
import type { TrackTapSources, PendingTrackTap, TrackTap } from './trackTap';
import {
    type Channels,
    collectionChannels,
    liftedChannels,
} from '../../shared/dsl/channels';

import z from 'zod';

export const PORT_MAX_LANES = 64;

/**
 * Scope with an optional source location captured at call time.
 * The base Scope type comes from Rust (napi); the extra field is
 * ignored by Rust but flows through to the renderer via IPC.
 */
export type ScopeWithLocation = Scope & {
    sourceLocation?: { line: number; column: number };
    /** Feeds an editor panel, never drawn as a scope view of its own. */
    headless?: boolean;
};

/** Options accepted by `.scope()`. */
export interface ScopeConfig {
    msPerFrame?: number;
    triggerThreshold?: number;
    triggerWaitToRender?: boolean;
    range?: [number, number];
    split?: ScopeSplit;
}

const ResolvedModuleOutput = z.object({
    lane: z.number().optional(),
    module: z.string(),
    port: z.string(),
    type: z.literal('cable'),
});

export type ResolvedModuleOutput = z.infer<typeof ResolvedModuleOutput>;

// Type definitions for Collection system
export type OrArray<T> = T | T[];
/** One lane: a constant, a note/Hz string, or a module output lane. */
export type Signal = number | string | ModuleOutput;
export type AnyCollection = BaseCollection<ModuleOutput>;
export type PolySignal = OrArray<Signal | AnyCollection>;

/** Groove template; mirrors Rust `GrooveTemplate` (camelCase). */
export interface GrooveTemplate {
    timing: number[];
    velocity?: number[];
    grid?: number;
    amount?: number;
    velocityAmount?: number;
    random?: number;
    seed?: number;
}

/**
 * A buffer output reference — returned by `$buffer()`, passed to readers
 * (like `$bufRead`, `$delayRead`) as their `buffer` param.
 */
export type BufferOutputRef = {
    type: 'buffer_ref';
    module: string;
    port: string;
    lanes: number;
    frameCount: number;
};

// ─── Cartesian product helpers ────────────────────────────────────────────────

export type ElementsOf<T extends unknown[][]> = {
    [K in keyof T]: T[K] extends (infer E)[] ? E : never;
};

/**
 * Compute the Cartesian product of the given arrays.
 *
 * Returns every possible combination of one element from each array.
 * Pairs well with the array overload of `.pipe()` to fan a signal across
 * multiple parameter dimensions.
 *
 * @param arrays - Zero or more arrays to combine
 * @returns Array of tuples, one per combination
 *
 * @example $cartesian([1, 2], ['a', 'b'])
 * // → [[1,'a'], [1,'b'], [2,'a'], [2,'b']]
 */
export function $cartesian<A extends unknown[][]>(
    ...arrays: A
): ElementsOf<A>[] {
    return arrays.reduce<unknown[][]>(
        (acc, arr) => acc.flatMap((combo) => arr.map((val) => [...combo, val])),
        [[]],
    ) as ElementsOf<A>[];
}

/**
 * A send to a bus, named by its track's name: the name alone sends at full
 * level, `[name, level]` at a linear `level` (5 = full, 0 = none).
 */
export type Send = TrackTap | [bus: TrackTap, level: PolySignal];

/** Options of `.track()`: how a track mixes into its output channels. */
export interface TrackOptions {
    /** Gain on the `.gain()` scale (5 = unity, 12 dB per step of 1), applied after the mixdown */
    gain?: PolySignal;
    /** Pan position (-5 = left, 0 = center, +5 = right). Default 0 */
    pan?: PolySignal;
    /** How far the lanes fan out across the stereo field (0 = none, 5 = edge to edge). Default 0 */
    spread?: Signal;
    /** Buses this track also feeds with its final signal */
    sends?: Send | Send[];
}

const TRACK_OPTION_NAMES = new Set(['gain', 'pan', 'spread', 'sends']);

/** A track as its label declares it. */
export interface TrackDeclaration {
    name: string;
    displayName: string;
    state: { mute: boolean; solo: boolean };
    channel: number;
    mono: boolean;
    /** The group track this track plays into instead of output channels. */
    group: string | null;
}

/** A `.scope()` on a track, drawn from the track's mixdown. */
interface TrackScope {
    config?: ScopeConfig;
    sourceLocation?: { line: number; column: number };
}

/** VU meter / mute / solo state shared by both out group kinds */
interface OutGroupVuState {
    /** The track's name: the meter's key and its hidden modules' id. */
    label: string;
    displayName: string;
    mute: boolean;
    solo: boolean;
    /** Source-order position across all out groups; assigned by addTrack */
    ordinal: number;
    /** Location of the track's label in the DSL source */
    sourceLocation?: { line: number; column: number };
    scopes: TrackScope[];
    /** Whether the label's value was a `.track(...)` result. */
    fromTrack: boolean;
    /** The group track this track plays into instead of output channels. */
    group: string | null;
    /** Whether this track is a group's: rooted at a `$group()`. */
    isGroup: boolean;
    /** Whether this track is a bus's: rooted at a `$bus()`. */
    isBus: boolean;
}

/** Internal storage for a stereo output group */
export interface StereoOutGroup extends OutGroupVuState {
    type: 'stereo';
    outputs: ModuleOutput[];
    gain?: PolySignal;
    pan?: PolySignal;
    spread?: PolySignal;
}

/** Internal storage for a mono output group */
export interface MonoOutGroup extends OutGroupVuState {
    type: 'mono';
    outputs: ModuleOutput[];
    gain?: PolySignal;
}

/** Each track name's binding, by the value the name reads as. */
const TRACK_BINDINGS = new WeakMap<object, string>();

/** Mark `value` as what track `name`'s name reads as. */
export function markTrackBinding(value: object, name: string): void {
    TRACK_BINDINGS.set(value, name);
}

/** The track name `value` is the binding of. */
function trackBindingName(value: unknown): string | undefined {
    return typeof value === 'object' && value !== null
        ? TRACK_BINDINGS.get(value)
        : undefined;
}

/**
 * Live-display tap for a locked VU control: the (module, port, lane)
 * address the engine samples so the panel can track the signal. Collections
 * and arrays tap their first lane. A deferred output's placeholder id is
 * resolved once every deferred is set (see `resolveControlTap`).
 */
function controlSourceTap(
    value: PolySignal | undefined,
): ControlTap | undefined {
    if (
        value == null ||
        typeof value === 'number' ||
        typeof value === 'string'
    ) {
        return undefined;
    }
    const single = toSignalArray(value)[0];
    if (single instanceof ModuleOutput) {
        return {
            lane: single.lane ?? 0,
            moduleId: single.moduleId,
            portName: single.portName,
        };
    }
    return undefined;
}

export type OutGroup = StereoOutGroup | MonoOutGroup;

interface SendGroup {
    outputs: ModuleOutput[];
    level: PolySignal;
}

/**
 * A send-return bus: `$bus()`. Its signal is one lane, the mix of every send
 * to it, known only once every track has run, so it is deferred until
 * `toPatch`. Sends name the bus by the track rooted at its signal.
 */
export class Bus {
    private builder: GraphBuilder;
    private sendGroups: SendGroup[] = [];
    private readonly mix: DeferredModuleOutput;
    readonly signal: Collection;
    /** The track rooted at this bus's signal. */
    trackName: string | undefined;

    constructor(builder: GraphBuilder) {
        this.builder = builder;
        this.mix = new DeferredModuleOutput(builder);
        this.signal = new Collection(this.mix);
        setSignalRoot(this.signal, this);
        builder.addBus(this);
    }

    addSend(outputs: ModuleOutput[], level: PolySignal): void {
        this.sendGroups.push({ level, outputs });
    }

    /** The module mixing the sends, once finalized; null with none. */
    mixModuleId(): string | null {
        return this.sendGroups.length === 0
            ? null
            : (this.mix.resolve()?.moduleId ?? null);
    }

    /** Build the mix of the sends and point the bus's signal at it. */
    finalize() {
        if (this.sendGroups.length === 0) {
            this.mix.set(0);
            return;
        }
        const mixed = this.builder.getFactory('$mix')(
            this.sendGroups.map((e) =>
                this.builder.getFactory('$amp')(
                    this.builder.$c(e.outputs),
                    e.level,
                ),
            ),
        );
        this.mix.set(this.builder.getFactory('$setLanes')(mixed, 1).items[0]);
    }
}

/**
 * A group: `$group()`. Its signal is one stereo lane, the sum of the final
 * output of every track whose label names the group's track, known only once
 * every track is built, so it is deferred until `toPatch`.
 */
export class Group {
    private builder: GraphBuilder;
    private members: ModuleOutput[][] = [];
    private readonly mix: DeferredModuleOutput;
    readonly signal: Collection;
    /** The track rooted at this group's signal. */
    trackName: string | undefined;

    constructor(builder: GraphBuilder) {
        this.builder = builder;
        this.mix = new DeferredModuleOutput(builder);
        this.signal = new Collection(this.mix);
        setSignalRoot(this.signal, this);
        builder.addGroup(this);
    }

    addMember(outputs: ModuleOutput[]): void {
        this.members.push(outputs);
    }

    /** Build the sum of the members and point the group's signal at it. */
    finalize() {
        if (this.members.length === 0) {
            this.mix.set(0);
            return;
        }
        const mixed = this.builder.getFactory('$mix')(this.members);
        this.mix.set(this.builder.getFactory('$setLanes')(mixed, 1).items[0]);
    }
}

/**
 * A signal with the options it mixes into the mixer with. It is not
 * a signal: a track is heard by labelling it, and only `.scope()` chains on
 * it, drawing the track's mixdown.
 */
export class Track {
    readonly source: unknown;
    readonly options: TrackOptions;
    readonly scopes: TrackScope[] = [];

    constructor(source: unknown, options: TrackOptions) {
        if (
            options === null ||
            typeof options !== 'object' ||
            Array.isArray(options)
        ) {
            throw new Error('.track takes an options object');
        }
        for (const key of Object.keys(options)) {
            if (!TRACK_OPTION_NAMES.has(key)) {
                throw new Error(
                    `.track has no \`${key}\` option: it takes gain, pan, spread and sends, and the track's label sets its name, mute, solo and channels`,
                );
            }
        }
        this.source = source;
        this.options = options;
    }

    /** Scope the track's mixdown: one lane, after its pan, spread and gain. */
    scope(config?: ScopeConfig): this {
        this.scopes.push({ config, sourceLocation: captureSourceLocation() });
        return this;
    }
}

/**
 * Normalize a {@link PolySignal} into a flat array of single-lane
 * {@link Signal}s: collections contribute their lanes, arrays flatten.
 */
function toSignalArray(v: PolySignal): Signal[] {
    if (Array.isArray(v)) {
        return v.flatMap(toSignalArray);
    }
    if (v instanceof BaseCollection) {
        return v.items;
    }
    return [v];
}

/**
 * BaseCollection is the DSL's only signal type: an ordered set of lanes,
 * each a {@link ModuleOutput}, with chainable DSP methods. Indexing and
 * iteration yield one-lane collections of the same class, so DSL code never
 * holds a bare lane; a one-lane collection is its own lane 0.
 */
export class BaseCollection<T extends ModuleOutput> {
    [index: number]: this;
    /** The lanes. Internal plumbing; DSL code sees only collections. */
    readonly items: T[] = [];
    private readonly laneViews: (this | undefined)[] = [];

    constructor(...args: T[]) {
        this.items.push(...args);
        for (let i = 0; i < args.length; i++) {
            Object.defineProperty(this, i, {
                get: () => this.lane(i),
            });
        }
    }

    /** Lane `i` as a one-lane collection of this collection's class. */
    private lane(i: number): this {
        if (this.items.length === 1) {
            return this;
        }
        let view = this.laneViews[i];
        if (view === undefined) {
            view = this.slice(i, i + 1);
            this.laneViews[i] = view;
        }
        return view;
    }

    get length(): number {
        return this.items.length;
    }

    *[Symbol.iterator](): Iterator<this> {
        for (let i = 0; i < this.items.length; i++) {
            yield this.lane(i);
        }
    }

    /**
     * A collection of the same class holding the items from `start` up to but
     * not including `end`, with `Array.prototype.slice` index semantics.
     */
    slice(start?: number, end?: number): this {
        const ctor = this.constructor as new (...args: T[]) => this;
        const sliced = new ctor(...this.items.slice(start, end));
        carrySignalRoot(sliced, this);
        const set = outputSetOf(this);
        if (set) {
            sliceRoot(sliced, set, this.items.length, start, end);
        }
        return sliced;
    }

    /**
     * Split into consecutive collections of the same class, each `size` items
     * long except the last, which holds the remainder.
     */
    chunk(size: number): this[] {
        if (!Number.isInteger(size) || size < 1) {
            throw new Error(
                `chunk() size must be a positive integer, got ${size}`,
            );
        }
        const chunks: this[] = [];
        for (let i = 0; i < this.items.length; i += size) {
            chunks.push(this.slice(i, i + size));
        }
        return chunks;
    }

    /**
     * The lanes of this collection followed by those of `other`, as a
     * collection of the same class. A multi-output module's outputs join
     * output by output, so the result is still a root standing for whichever
     * fits the input it feeds. A plain receiver takes a multi-output argument
     * as the output it stands for read as a signal.
     */
    concat(other: this): this {
        const that = patternSignal(other);
        const ctor = this.constructor as new (...args: T[]) => this;
        if (
            !(that instanceof BaseCollection) ||
            !(that instanceof ctor || this.constructor === Collection)
        ) {
            throw new Error(
                `concat() takes a signal of the same type as the one it is called on`,
            );
        }
        const joined = new ctor(...this.items, ...(that.items as T[]));
        concatRoots(joined, this, that);
        carrySignalRoot(joined, this);
        return joined;
    }

    /**
     * Add scope visualization for all outputs in the collection. On a
     * multi-output root it returns a copy of the root, and the scope draws
     * the output that copy is plugged in as.
     */
    scope(config?: ScopeConfig): this {
        if (this.items.length === 0) {
            return this;
        }
        const loc = captureSourceLocation();
        const { builder } = this.items[0];
        const set = outputSetOf(this);
        if (!set) {
            builder.addScope(this.items, config, loc);
            return this;
        }
        const ctor = this.constructor as new (...args: T[]) => this;
        const scoped = new ctor(...this.items);
        carrySignalRoot(scoped, this);
        carryDrySource(scoped, this);
        const scopedSet: OutputSet = {
            outputs: set.outputs,
            parents: [set],
            values: set.values,
        };
        markRoot(scoped, scopedSet);
        builder.scopeRoot(scopedSet, this.items, config, loc);
        return scoped;
    }

    /**
     * How this signal mixes into its track: gain, pan, spread and sends. A
     * track is heard once labelled (`bass: sig.track({ pan: -2 })`).
     */
    track(options: TrackOptions = {}): Track {
        return new Track(this, options);
    }

    pipe<U>(pipelineFunc: (self: this) => U): U;
    pipe<U extends PolySignal, E>(
        pipelineFunc: (self: this, item: E) => U,
        array: E[],
    ): Collection;
    pipe<U>(
        pipelineFunc: (self: this, ...args: unknown[]) => U,
        ...arrays: unknown[][]
    ): U | Collection {
        if (arrays.length === 0) {
            return pipelineFunc(this);
        }
        return this.items[0].builder.$c(
            ...arrays[0].map((item) => pipelineFunc(this, item) as PolySignal),
        );
    }

    pipeMix(
        pipelineFunc: (self: this) => AnyCollection,
        mix: PolySignal = 2.5,
    ): Collection {
        const result = pipelineFunc(this);
        const mixed = crossfadeMix(this.items[0].builder, this, result, mix);
        carrySignalRoot(mixed, this);
        return mixed;
    }

    /**
     * Crossfade the signal this collection was chained from against it:
     * `sig.lpf('800hz').wet(2.5)` (0 = dry, 5 = wet, 2.5 = equal).
     */
    wet(mix: PolySignal = 2.5): Collection {
        const builder = this.items[0]?.builder;
        if (!builder) {
            return new Collection();
        }
        const mixed = wetMix(builder, this, mix);
        carrySignalRoot(mixed, this);
        return mixed;
    }

    /** A one-lane collection reads as its lane, so it interpolates into patterns. */
    toString(): string {
        if (this.items.length === 1) {
            return this.items[0].toString();
        }
        return `[${this.items.map((item) => item.toString()).join(',')}]`;
    }
}

/**
 * Make `sliced` a root standing for the same lanes of every output of the
 * root `set` came from, so each input still takes the output that fits it:
 * `$perc(seq[1])` takes lane 1 of the trigger. An output whose width
 * differs from the root's, such as a fixed-width one, stays whole.
 */
function sliceRoot(
    sliced: object,
    set: OutputSet,
    rootLanes: number,
    start?: number,
    end?: number,
): void {
    const indices = Array.from({ length: rootLanes }, (_, i) => i).slice(
        start,
        end,
    );
    const values: Record<string, unknown> = {};
    for (const [name, value] of Object.entries(set.values)) {
        values[name] =
            value instanceof BaseCollection && value.length === rootLanes
                ? new (
                      value.constructor as new (
                          ...args: ModuleOutput[]
                      ) => AnyCollection
                  )(...indices.map((i) => value.items[i]))
                : value;
    }
    markRoot(sliced, {
        outputs: set.outputs,
        parents: [set],
        values,
    });
}

/**
 * Make `joined` a root standing for the outputs of the root `a` and those of
 * `b`, each output's lanes those of `a` followed by those of `b`. When `a` is
 * no root, `b` joins as the output it stands for read as a signal.
 */
function concatRoots(joined: object, a: object, b: object): void {
    const set = outputSetOf(a);
    if (!set) {
        return;
    }
    const otherSet = outputSetOf(b);
    const other = otherSet?.values ?? {};
    const names = Object.keys(set.values);
    const otherNames = Object.keys(other);
    if (names.join() !== otherNames.join()) {
        const list = (ns: string[]) =>
            ns.length === 0 ? 'none' : ns.map((n) => `_${n}`).join(', ');
        throw new Error(
            `concat() takes a signal of the same type as the one it is called on: one has ${list(names)}, the other ${list(otherNames)}`,
        );
    }
    const values: Record<string, unknown> = {};
    for (const name of names) {
        const value = set.values[name] as AnyCollection;
        values[name] = value.concat(other[name] as AnyCollection);
    }
    markRoot(joined, {
        outputs: set.outputs,
        parents: otherSet ? [set, otherSet] : [set],
        values,
    });
}

/**
 * The same lanes as a ranged collection, for outputs whose source is set
 * later to one the engine bounds (a `$trackTap` control).
 */
export function asRanged(
    collection: BaseCollection<ModuleOutput>,
): CollectionWithRange {
    for (const { builder, moduleId, portName } of collection.items) {
        builder.markRanged(moduleId, portName);
    }
    return new CollectionWithRange(...collection.items);
}

/**
 * Remap `items` onto `outMin`…`outMax` through `$remap`, from `inMin`…`inMax`
 * or, left out, from the range the engine knows for them (±5 for a signal
 * without one).
 */
function remapRange(
    items: ModuleOutput[],
    outMin: PolySignal,
    outMax: PolySignal,
    inMin?: PolySignal,
    inMax?: PolySignal,
): CollectionWithRange {
    if (items.length === 0) {
        return new CollectionWithRange();
    }
    return items[0].builder.getFactory('$remap')(
        items,
        outMin,
        outMax,
        inMin,
        inMax,
    ) as CollectionWithRange;
}

/**
 * Collection of ModuleOutput instances.
 * Use .range(outMin, outMax, inMin, inMax) to remap with explicit input range.
 */
export class Collection extends BaseCollection<ModuleOutput> {
    /**
     * Remap outputs from explicit input range to output range
     */
    range(
        outMin: PolySignal,
        outMax: PolySignal,
        inMin: PolySignal,
        inMax: PolySignal,
    ): CollectionWithRange {
        return remapRange(this.items, outMin, outMax, inMin, inMax);
    }
}

/**
 * A collection whose range the engine knows: `.range(outMin, outMax)` maps
 * from it.
 */
export class CollectionWithRange extends BaseCollection<ModuleOutput> {
    /**
     * Remap outputs onto `outMin`…`outMax`, from their known range or from
     * `inMin`…`inMax` when given.
     */
    range(
        outMin: PolySignal,
        outMax: PolySignal,
        inMin?: PolySignal,
        inMax?: PolySignal,
    ): CollectionWithRange {
        return remapRange(this.items, outMin, outMax, inMin, inMax);
    }
}

/** The lanes of every ranged collection given, as one ranged collection. */
export const $r = (
    ...args: OrArray<CollectionWithRange>[]
): CollectionWithRange =>
    new CollectionWithRange(
        ...args.flat().flatMap((arg) => {
            if (!(arg instanceof CollectionWithRange)) {
                throw new Error(
                    '$r: every argument must be a ranged collection',
                );
            }
            return arg.items;
        }),
    );

/**
 * Broadcast a signal across `lanes` lanes by cycling its existing
 * lanes (lane `i` ← source lane `i % len`). Useful when a narrow
 * input must excite a wider polyphonic structure: summing/buffering modules
 * skip a narrower input group's missing lanes rather than cycling them, so
 * the extra lanes would otherwise be silent. Returns the signal unchanged
 * when it carries no lanes.
 */
export const cycleToLanes = (
    signal: AnyCollection,
    lanes: number,
): AnyCollection => {
    const { items } = signal;
    if (items.length === 0) return signal;
    return new Collection(
        ...Array.from({ length: lanes }, (_, i) => items[i % items.length]),
    );
};

/**
 * Factory function type for creating modules via DSL.
 * Returns the module's output(s) directly rather than the ModuleNode.
 */
export type FactoryFunction = (
    ...args: unknown[]
) => Collection | CollectionWithRange;

/**
 * Source location information for mapping validation errors back to DSL code.
 */
export interface SourceLocation {
    /** 1-based line number in the DSL source */
    line: number;
    /** 1-based column number in the DSL source */
    column: number;
    /** Whether the module ID was explicitly set by the user */
    idIsExplicit: boolean;
}

/**
 * GraphBuilder manages the construction of a PatchGraph from DSL code.
 * It tracks modules, generates deterministic IDs, and builds the final graph.
 *
 * Note: Factory functions add overhead from lane count derivation but provide
 * consistency across all module creation paths.
 */
export class GraphBuilder implements ChainHost {
    /** Resolves multi-output module roots and records each choice. */
    readonly outputResolver = new OutputResolver();
    private modules = new Map<string, ModuleSpec>();
    /** The output ports handed out as ranged, as `module:port`. */
    private rangedPorts = new Set<string>();
    /** Each module's channels as the DSL settles them; unlisted is mono. */
    private moduleChannels = new Map<string, Channels>();
    private counters = new Map<string, number>();
    private schemas: ProcessedModuleSchema[] = [];
    private schemaByName = new Map<string, ProcessedModuleSchema>();
    /** Chain methods backed by a DSL function rather than a schema's factory (`delay`). */
    private chainHelpers = new Map<string, ChainFunction>();
    private scopes: ScopeWithLocation[] = [];
    /** The roots among the scoped values, which draw the output they are taken as. */
    private scopeRoots = new Map<ScopeWithLocation, OutputSet>();
    /** `ui: true` EQ panels, collected by `toPatch`. */
    private eqPanels: EqPanelDef[] = [];
    /** Output groups keyed by baseChannel */
    private outGroups = new Map<number, OutGroup[]>();
    /** Monotonic source-order counter for out groups */
    private outGroupOrdinal = 0;
    private factoryRegistry = new Map<string, FactoryFunction>();
    private sourceLocationMap = new Map<string, SourceLocation>();
    /** Track all deferred outputs for string replacement during toPatch */
    private deferredOutputs = new Map<string, DeferredModuleOutput>();
    /** `$trackTap` taps, resolved against their track by name in toPatch */
    private trackTaps: {
        label: string;
        tap: PendingTrackTap;
        sourceLocation?: { line: number; column: number };
    }[] = [];
    /** Global tempo for ROOT_CLOCK in BPM (default: 120) */
    private tempo: number = 120;
    /** Whether $setTempo was explicitly called in the DSL */
    private tempoExplicitlySet: boolean = false;
    /** Main gain signal. */
    private mainGain: PolySignal = DEFAULT_OUTPUT_GAIN;
    /** Time signature numerator (beats per bar) for ROOT_CLOCK */
    private timeSignatureNumerator: number = 4;
    /** Time signature denominator (beat value) for ROOT_CLOCK */
    private timeSignatureDenominator: number = 4;
    /** Global groove template for ROOT_CLOCK; straight when unset */
    private groove: GrooveTemplate | undefined = undefined;
    private busses: Bus[] = [];
    /** Each labelled bus, by its track's name. */
    private bussesByName = new Map<string, Bus>();
    private groups: Group[] = [];
    /** Each labelled group, by its track's name. */
    private groupsByName = new Map<string, Group>();
    /**
     * Each track's `sends` option, settled in toPatch: which names are buses
     * is known only once every label has run.
     */
    private pendingSends: {
        track: string;
        sends: unknown;
        sourceLocation?: { line: number; column: number };
    }[] = [];
    private endOfChainCb: (
        mixed: Collection,
    ) => Collection | CollectionWithRange = (e) => e;
    private processingEndOfChain: boolean = false;

    constructor(schemas: ModuleSchema[]) {
        this.schemas = processSchemas(schemas);
        this.schemaByName = new Map(this.schemas.map((s) => [s.name, s]));
        chains.addModules(this.schemas);
    }

    /** The function behind chain path `path`: `lpf` is the `$lpf` factory. */
    chainFunction(path: string): ChainFunction {
        return (
            this.chainHelpers.get(path) ??
            (this.getFactory(`$${path}`) as ChainFunction)
        );
    }

    /**
     * Make DSL helper `fn` chain method `name`. It takes the chained signal
     * first, as a module factory does.
     */
    registerChainMethod(name: string, fn: ChainFunction): void {
        this.chainHelpers.set(name, fn);
        chains.addMethod(name);
    }

    /**
     * Generate a deterministic ID for a module type
     */
    private generateId(moduleType: string, explicitId?: string): string {
        if (explicitId) {
            return explicitId;
        }

        let counter = (this.counters.get(moduleType) || 0) + 1;
        let id = `${moduleType}-${counter}`;

        // If the generated ID is already taken (e.g. by an explicit ID),
        // Keep incrementing until we find a free one.
        while (this.modules.has(id)) {
            counter++;
            id = `${moduleType}-${counter}`;
        }

        this.counters.set(moduleType, counter);
        return id;
    }

    private addModule(
        moduleType: string,
        explicitId?: string,
        sourceLocation?: { line: number; column: number },
    ): ModuleNode {
        const id = this.generateId(moduleType, explicitId);

        if (this.modules.has(id)) {
            throw new Error(`Duplicate module id: ${id}`);
        }

        // Check if module type exists in schemas
        const schema = this.schemaByName.get(moduleType);
        if (!schema) {
            throw new Error(`Unknown module type: ${moduleType}`);
        }

        // Store source location for error mapping
        if (sourceLocation) {
            this.sourceLocationMap.set(id, {
                column: sourceLocation.column,
                idIsExplicit: Boolean(explicitId),
                line: sourceLocation.line,
            });
        }

        const moduleState: ModuleSpec = {
            id,
            idIsExplicit: Boolean(explicitId),
            moduleType,
            params: {},
        };

        this.modules.set(id, moduleState);
        return new ModuleNode(this, id, moduleType, schema);
    }

    /**
     * Add a module and finish it with `build`. If `build` throws, the module
     * leaves no trace in the graph, so a script that catches the error
     * patches as though the call never ran.
     */
    buildModule<T>(
        moduleType: string,
        explicitId: string | undefined,
        sourceLocation: { line: number; column: number } | undefined,
        build: (node: ModuleNode) => T,
    ): T {
        const previousCounter = this.counters.get(moduleType);
        const node = this.addModule(moduleType, explicitId, sourceLocation);
        const counterAfterAdd = this.counters.get(moduleType);
        try {
            return build(node);
        } catch (error) {
            const { id } = node;
            this.modules.delete(id);
            this.sourceLocationMap.delete(id);
            this.moduleChannels.delete(id);
            for (const port of [...this.rangedPorts]) {
                if (port.startsWith(`${id}:`)) {
                    this.rangedPorts.delete(port);
                }
            }
            if (this.counters.get(moduleType) === counterAfterAdd) {
                if (previousCounter === undefined) {
                    this.counters.delete(moduleType);
                } else {
                    this.counters.set(moduleType, previousCounter);
                }
            }
            throw error;
        }
    }

    /**
     * Get a module by ID
     */
    getModule(id: string): ModuleSpec | undefined {
        return this.modules.get(id);
    }

    /** The schema of module `id`'s type. */
    schemaOf(id: string): ProcessedModuleSchema | undefined {
        const module = this.modules.get(id);
        return module && this.schemaByName.get(module.moduleType);
    }

    /** Set the channels module `id`'s lane-following outputs carry. */
    setModuleChannels(id: string, channels: Channels): void {
        this.moduleChannels.set(id, channels);
    }

    /** The channels each lane of output `port` of module `id` carries. */
    private portChannels(id: string, port: string): Channels {
        // A `mono` output carries one value per lane whatever its module.
        const spec = this.modules.get(id);
        const output = spec
            ? this.schemaByName
                  .get(spec.moduleType)
                  ?.outputs.find((o) => o.name === port)
            : undefined;
        return output?.mono ? 'mono' : (this.moduleChannels.get(id) ?? 'mono');
    }

    /**
     * The channels each lane of `value` carries: an output or collection, an
     * array or `$gN` group of them, a `$buffer` reference, which carries
     * what its buffer records, or anything holding them. A deferred lane is
     * unknown until the engine settles it.
     */
    private *laneChannels(value: unknown): Generator<Channels> {
        if (value instanceof DeferredModuleOutput) {
            yield 'unknown';
        } else if (value instanceof ModuleOutput) {
            yield this.portChannels(value.moduleId, value.portName);
        } else if (value instanceof BaseCollection) {
            for (const item of value.items) {
                yield* this.laneChannels(item);
            }
        } else if (Array.isArray(value)) {
            for (const item of value) {
                yield* this.laneChannels(item);
            }
        } else if (isSignalGroupLike(value)) {
            yield* this.laneChannels(value.signals);
        } else if (isPlainObject(value)) {
            if (
                value.type === 'buffer_ref' &&
                typeof value.module === 'string'
            ) {
                yield this.moduleChannels.get(value.module) ?? 'mono';
                return;
            }
            for (const field of Object.values(value)) {
                yield* this.laneChannels(field);
            }
        }
    }

    /** The channels of `value`: mono or stereo when every lane is. */
    channelsOf(value: unknown): Channels {
        return collectionChannels(this.laneChannels(value));
    }

    /** The channels a lifted module reading `inputs` lane by lane settles to. */
    liftedChannelsOf(inputs: Iterable<unknown>): Channels {
        const lanes = this.laneChannels.bind(this);
        return liftedChannels(
            (function* () {
                for (const input of inputs) {
                    yield* lanes(input);
                }
            })(),
        );
    }

    /** Record that module `moduleId`'s `port` is ranged. */
    markRanged(moduleId: string, port: string): void {
        this.rangedPorts.add(`${moduleId}:${port}`);
    }

    /**
     * Whether module `moduleId` declares its range through its own params
     * (`$signal`'s `min` and `max`), whatever its input: both set, and apart
     * when both are numbers.
     */
    declaresRange(moduleId: string): boolean {
        const module = this.modules.get(moduleId);
        const declared =
            module && this.schemaByName.get(module.moduleType)?.declaredRange;
        if (!declared) {
            return false;
        }
        const [min, max] = declared.map((name) => module.params[name]);
        return (
            min !== undefined &&
            max !== undefined &&
            !(typeof min === 'number' && min === max)
        );
    }

    /**
     * How a param value, as stored, stands towards the range the engine knows
     * for it, lane by lane: `'constant'` holds still in every lane
     * (an unset param reads one), `'ranged'` is a cable from a ranged port in
     * every lane, `'carried'` mixes the two, and `'unranged'` has a cable
     * from any other port, a deferred output's among them.
     */
    rangeOf(value: unknown): 'constant' | 'ranged' | 'carried' | 'unranged' {
        if (
            value === undefined ||
            typeof value === 'number' ||
            typeof value === 'string'
        ) {
            return 'constant';
        }
        if (Array.isArray(value)) {
            const kinds = new Set(value.map((item) => this.rangeOf(item)));
            if (kinds.has('unranged')) {
                return 'unranged';
            }
            if (kinds.size === 1) {
                return [...kinds][0];
            }
            return kinds.size === 0 ? 'constant' : 'carried';
        }
        if (
            isPlainObject(value) &&
            value.type === 'cable' &&
            this.rangedPorts.has(
                `${String(value.module)}:${String(value.port)}`,
            )
        ) {
            return 'ranged';
        }
        return 'unranged';
    }

    /**
     * Set a parameter value for a module
     */
    setParam(moduleId: string, paramName: string, value: unknown): void {
        const module = this.modules.get(moduleId);
        if (!module) {
            throw new Error(`Module not found: ${moduleId}`);
        }
        // Every param write funnels through here, so this is the one place
        // that can see a $gN wrapper misplaced below the root of its param.
        assertSignalGroupsTopLevelOnly(value);
        module.params[paramName] = value;
    }

    /**
     * Register factory functions for late binding.
     * Called by DSLContext after factory creation to enable internal factory usage.
     */
    setFactoryRegistry(factories: Map<string, FactoryFunction>): void {
        this.factoryRegistry = factories;
    }

    /**
     * Set the global tempo for ROOT_CLOCK
     * @param tempo - Tempo in BPM (plain number)
     */
    setTempo(tempo: number): void {
        this.tempo = tempo;
        this.tempoExplicitlySet = true;
    }

    /**
     * Set the main gain
     * @param gain - Signal value for main gain (5, unity, is the default; 12 dB per step of 1)
     */
    setMainGain(gain: PolySignal): void {
        this.mainGain = gain;
    }

    /**
     * Set the time signature for ROOT_CLOCK
     * @param numerator - Beats per bar (positive integer)
     * @param denominator - Beat value (positive integer)
     */
    setTimeSignature(numerator: number, denominator: number): void {
        this.timeSignatureNumerator = numerator;
        this.timeSignatureDenominator = denominator;
    }

    /**
     * Set the global groove ROOT_CLOCK reports to every $cycle and $drumMap
     */
    setGroove(template: GrooveTemplate): void {
        this.groove = template;
    }

    setEndOfChainCb(
        cb: (mixed: Collection) => Collection | CollectionWithRange,
    ): void {
        if (this.processingEndOfChain) {
            throw new Error(
                '`$setEndOfChainCb` is not allowed in its own callback.',
            );
        }
        this.endOfChainCb = cb;
    }

    /**
     * Get a factory function by module type name.
     * Returns undefined if factories haven't been registered yet.
     */
    getFactory(moduleType: string): FactoryFunction {
        const factory = this.factoryRegistry.get(moduleType);
        if (!factory) {
            throw new Error(`Factory ${moduleType} not found`);
        }
        return factory;
    }

    /**
     * Create a Collection from outputs. Each argument is flattened into the
     * Collection's lanes: collections contribute their lanes, arrays
     * are spread, and bare Signal literals — numbers and
     * note/Hz strings — are lifted into `$signal` modules, so `$c(440, 'c4',
     * osc)` works alongside `$c(osc1, osc2)`. Lifting nests, so scalars inside
     * an array argument are handled too. Lifting a scalar needs the `$signal`
     * factory, hence a builder method.
     */
    $c(...args: PolySignal[]): Collection {
        // Resolved on the first scalar: a Collection of only ModuleOutputs
        // needs no factory.
        let signal: FactoryFunction | undefined;
        const lift = (value: unknown): ModuleOutput[] => {
            // Scalar literal checked before the iterable branch so a string is
            // one signal rather than spread into characters.
            if (typeof value === 'number' || typeof value === 'string') {
                signal ??= this.getFactory('$signal');
                return signal(value).items;
            }
            if (value instanceof ModuleOutput) {
                return [value];
            }
            if (value instanceof BaseCollection) {
                return value.items;
            }
            if (
                typeof value !== 'object' ||
                value === null ||
                !(Symbol.iterator in value)
            ) {
                throw new Error(
                    `$c: cannot make a collection from ${value === null ? 'null' : typeof value}`,
                );
            }
            return [...(value as Iterable<unknown>)].flatMap(lift);
        };
        return new Collection(...args.flatMap(lift));
    }

    /**
     * Build the final PatchGraph
     *
     * Note: Uses factory functions for signal/mix modules for consistency,
     * which adds overhead from lane count derivation on every patch build.
     */
    toPatch(): PatchGraph {
        // A send carries its track's final signal through the mixer stage of
        // every group it plays into, so sends are added once every track's
        // stage is built, and the buses are finalized after.
        const sendsByTrack = new Map(
            this.pendingSends.map(({ track, sends, sourceLocation }) => [
                track,
                this.resolveSends(track, sends, sourceLocation),
            ]),
        );

        const signalFactory = this.getFactory('$signal');
        const mixFactory = this.getFactory('$mix');
        const panFactory = this.getFactory('$pan');
        const setLanesFactory = this.getFactory('$setLanes');
        const monoFactory = this.getFactory('$mono');
        const leftFactory = this.getFactory('$left');
        const rightFactory = this.getFactory('$right');
        const ampFactory = this.getFactory('$amp');
        const gainFactory = this.getFactory('$gain');
        const slewFactory = this.getFactory('$slew');
        const smoothControlFactory = this.getFactory('_smoothControl');

        // VU meter entries, one per out group, collected in compile order and
        // sorted into source order before emission.
        const vuMeters: (VuMeterDef & { ordinal: number })[] = [];

        /**
         * A track's mixer stage: its mixdown, pan and gain, then its mute/solo
         * gate.
         */
        interface MixStage {
            type: OutGroup['type'];
            pan: PolySignal;
            spread: PolySignal;
            gain: PolySignal;
            gate: Collection;
        }
        // A stereo stage pans its lanes and sums them into one stereo lane;
        // a centred lane plays at unity on both sides, as on a DAW track. A
        // mono stage sums its lanes folded. The result is post-gain,
        // pre-gate, which is where a track's meter reads, like a post-fader
        // DAW meter.
        const gainStage = (
            outputs: ModuleOutput[],
            stage: Omit<MixStage, 'gate'>,
            tapId?: string,
        ): ModuleOutput[] => {
            const mixed =
                stage.type === 'stereo'
                    ? setLanesFactory(
                          panFactory(outputs, stage.pan, {
                              spread: stage.spread,
                          }),
                          1,
                      ).items
                    : setLanesFactory(monoFactory(outputs), 1).items;
            return gainFactory(
                mixed,
                stage.gain,
                tapId === undefined ? undefined : { id: tapId },
            ).items;
        };
        const gated = (signal: ModuleOutput[], gate: Collection) =>
            (ampFactory(signal, gate) as Collection).items;
        // Each track's stage and the group it plays into, by name, and each
        // sending track's final signal.
        const stagesByLabel = new Map<
            string,
            { stage: MixStage; group: string | null }
        >();
        const sending: {
            label: string;
            sk: string;
            outputs: ModuleOutput[];
            meter: VuMeterDef;
        }[] = [];
        // Each locked control's live tap, resolved once every deferred is set.
        const controlTaps = new Map<VuControl, ControlTap>();
        /**
         * A mixer control for `value` (`fallback` when absent): a number is
         * lifted into a `_smoothControl` with id `id`, which the panel drives
         * live; a signal locks the control, which the panel follows.
         */
        const liftControl = (
            value: PolySignal | undefined,
            fallback: number,
            range: ControlRange,
            id: string,
        ): { control: VuControl; signal: PolySignal } => {
            const v = value ?? fallback;
            if (typeof v === 'number') {
                return {
                    control: { locked: false, moduleId: id, value: v },
                    signal: smoothControlFactory(v, { ...range, id }),
                };
            }
            const control: VuControl = {
                locked: true,
                moduleId: null,
                value: null,
            };
            const tap = controlSourceTap(v);
            if (tap) {
                controlTaps.set(control, tap);
            }
            return { control, signal: v };
        };

        // Each track's tap, for the check that no track reads a bus.
        const feedbackTracks: FeedbackTrack[] = [];

        // Each track's signals, by name, for `$trackTap` taps.
        const outsByLabel = new Map<
            string,
            {
                audio: ModuleOutput[];
                active: ModuleOutput;
                level: PolySignal;
                pan: PolySignal;
                isBus: boolean;
            }
        >();

        // Process output groups and build channel collections
        if (this.outGroups.size > 0) {
            // Collect all channel collections to mix together
            const allChannelCollections: (ModuleOutput | number)[][] = [];

            // Sort by baseChannel for deterministic processing
            const sortedChannels = [...this.outGroups.keys()].sort(
                (a, b) => a - b,
            );

            const allGroups = [...this.outGroups.values()]
                .flat()
                .sort((a, b) => a.ordinal - b.ordinal);

            const gates = trackGates(
                allGroups.map((g) => ({
                    group: g.group,
                    isBus: g.isBus,
                    key: g.label,
                    mute: g.mute,
                    solo: g.solo,
                })),
            );

            // Module-id suffix per group, assigned in source order. Keys are
            // track names, unique and never '__main__' (a name is an
            // identifier); suffixes seed the hidden __vu* module ids, so they
            // are unique too — 'main' is reserved for the main gain stage,
            // and names that sanitize alike get a numeric disambiguator.
            const skByOrdinal = new Map<number, string>();
            const skByLabel = new Map<string, string>();
            const usedSk = new Set<string>(['main']);
            for (const group of allGroups) {
                const skBase = group.label.replace(/[^a-zA-Z0-9_]/g, '_');
                let sk = skBase;
                for (let n = 2; usedSk.has(sk); n++) {
                    sk = `${skBase}_${n}`;
                }
                usedSk.add(sk);
                skByOrdinal.set(group.ordinal, sk);
                skByLabel.set(group.label, sk);
            }

            for (const baseChannel of sortedChannels) {
                const groups = this.outGroups.get(baseChannel)!;

                for (const group of groups) {
                    const key = group.label;
                    // Sanitized id suffix for the group's deterministic
                    // __vuTap_/__vuMute_ module ids.
                    const sk = skByOrdinal.get(group.ordinal)!;
                    const tapId = `__vuTap_${sk}`;

                    // The gain stage always compiles, so the fader has a
                    // target; absent gain seeds unity.
                    const gain = liftControl(
                        group.gain,
                        UNITY_OUT_GAIN,
                        OUT_GAIN_RANGE,
                        `__vuGain_${sk}`,
                    );
                    // A signal gain is averaged to one lane, so the track stays
                    // one lane wide, and a mono track's is folded with it.
                    const gainSource =
                        gain.control.locked && typeof gain.signal !== 'number'
                            ? setLanesFactory(
                                  group.type === 'stereo'
                                      ? gain.signal
                                      : monoFactory(gain.signal),
                                  1,
                                  { mode: 'average' },
                              )
                            : gain.signal;
                    // A mono track has no pan knob.
                    const pan =
                        group.type === 'stereo'
                            ? liftControl(
                                  group.pan,
                                  0,
                                  OUT_PAN_RANGE,
                                  `__vuPan_${sk}`,
                              )
                            : null;
                    const panSource: PolySignal = pan ? pan.signal : 0;

                    const levels = {
                        gain: gainSource,
                        pan: panSource,
                        spread:
                            group.type === 'stereo' ? (group.spread ?? 0) : 0,
                        type: group.type,
                    };
                    const tapOutputs = gainStage(group.outputs, levels, tapId);
                    feedbackTracks.push({
                        isBus: group.isBus,
                        label: group.label,
                        sourceLocation: group.sourceLocation,
                        tapId,
                    });

                    // Mute/solo gate stage. The gate $signal holds 5 (unity
                    // for `$amp`) or 0 (silence); the VU panel
                    // flips it live via the single-module update path, and the
                    // $slew declicks the 5 V swing over ~10 ms.
                    const gate = gates.get(group.label) ? 5 : 0;
                    const muteSig = signalFactory(gate, {
                        ...GATE_RANGE,
                        id: `__vuMute_${sk}`,
                    }) as Collection;
                    const smoothed = slewFactory(muteSig, {
                        fall: 0.002,
                        id: `__vuSlew_${sk}`,
                        rise: 0.002,
                    }) as Collection;
                    // The gate must sit at this patch's mute state from a
                    // swap's first sample — a carried-over open gate would
                    // leak an on-bar attack while still closing. Live M/S
                    // toggles replace only the $signal, so they still ramp
                    // through this slew; the engine's swap declick smooths
                    // the snap for sustained tones.
                    const slewSpec = this.modules.get(`__vuSlew_${sk}`);
                    if (slewSpec) {
                        slewSpec.skipStateTransfer = true;
                    }
                    const outputSignals = gated(tapOutputs, smoothed);
                    const stage: MixStage = { ...levels, gate: smoothed };
                    stagesByLabel.set(group.label, {
                        group: group.group,
                        stage,
                    });

                    outsByLabel.set(group.label, {
                        active: muteSig.items[0],
                        audio: outputSignals,
                        isBus: group.isBus,
                        // The tap reads the gain as written, ranged as the
                        // user's own signal is.
                        level: gain.control.locked ? gain.signal : gainSource,
                        pan: panSource,
                    });

                    for (const { config, sourceLocation } of group.scopes) {
                        this.addScope(tapOutputs, config, sourceLocation);
                    }

                    const meter: VuMeterDef & { ordinal: number } = {
                        baseChannel,
                        channels: group.type === 'stereo' ? 2 : 1,
                        fromTrack: group.fromTrack,
                        group: group.group,
                        isGroup: group.isGroup,
                        isBus: group.isBus,
                        gain: gain.control,
                        key,
                        label: group.displayName,
                        moduleId: tapId,
                        mute: group.mute,
                        muteModuleId: `__vuMute_${sk}`,
                        ordinal: group.ordinal,
                        pan: pan?.control ?? null,
                        portName: 'output',
                        solo: group.solo,
                        sourceLocation: group.sourceLocation,
                    };
                    vuMeters.push(meter);
                    if (!group.isBus) {
                        sending.push({
                            label: group.label,
                            meter,
                            outputs: outputSignals,
                            sk,
                        });
                    }

                    // A track with a group plays into it and nowhere else.
                    if (group.group !== null) {
                        const target = this.groupsByName.get(group.group);
                        if (!target) {
                            throw new Error(
                                `track ${group.label} cannot play into ${group.group}, which is not a group. A group is a track whose signal is $group() or methods chained on it${lineSuffix(group.sourceLocation)}`,
                            );
                        }
                        target.addMember(outputSignals);
                        continue;
                    }

                    // Build channel collection with baseChannel silent channels prepended
                    const channelCollection: (ModuleOutput | number)[] = [];

                    // Add silent channels for baseChannel offset
                    for (let i = 0; i < baseChannel; i++) {
                        // Push 0 (Signal::Volts(0.0)) to represent silence
                        channelCollection.push(0);
                    }

                    // A stereo out's sides go to its two device channels.
                    if (group.type === 'stereo') {
                        channelCollection.push(
                            ...leftFactory(outputSignals).items,
                            ...rightFactory(outputSignals).items,
                        );
                    } else {
                        channelCollection.push(...outputSignals);
                    }

                    allChannelCollections.push(channelCollection);
                }
            }
            for (const group of this.groups) {
                group.finalize();
            }
            // Every track sends to every bus, at 0 where its source names
            // none, so the mixer's send dial plays from its first move. A
            // numeric level is lifted into a _smoothControl the dial drives.
            const sendBuses = this.busses.filter(
                (b) => b.trackName !== undefined,
            );
            const sendIds = new Set<string>();
            for (const { label, outputs, sk, meter } of sendBuses.length > 0
                ? sending
                : []) {
                let sent = outputs;
                for (
                    let parent = stagesByLabel.get(label)!.group;
                    parent !== null;
                    parent = stagesByLabel.get(parent)!.group
                ) {
                    const { stage } = stagesByLabel.get(parent)!;
                    sent = gated(gainStage(sent, stage), stage.gate);
                }
                const named = sendsByTrack.get(label) ?? [];
                meter.sends = sendBuses.map((bus): VuSendDef => {
                    const busName = bus.trackName!;
                    const levels = named
                        .filter(([target]) => target === bus)
                        .map(([, level]) => level ?? SEND_LEVEL_RANGE.max);
                    // Two sends to one bus have no one level to set.
                    if (levels.length > 1) {
                        for (const each of levels) {
                            bus.addSend(sent, each);
                        }
                        return {
                            bus: busName,
                            locked: true,
                            moduleId: null,
                            value: null,
                        };
                    }
                    const idBase = `__vuSend_${sk}_${skByLabel.get(busName)}`;
                    let id = idBase;
                    for (let n = 2; sendIds.has(id); n++) {
                        id = `${idBase}_${n}`;
                    }
                    sendIds.add(id);
                    const { control, signal } = liftControl(
                        levels[0],
                        0,
                        SEND_LEVEL_RANGE,
                        id,
                    );
                    bus.addSend(sent, signal);
                    // The same object, which its live tap is keyed by.
                    return Object.assign(control, { bus: busName });
                });
            }
            for (const bus of this.busses) {
                bus.finalize();
            }
            // Mix all channel collections together using poly mix
            // Each collection contributes to corresponding output channels
            const finalMix = mixFactory(allChannelCollections) as Collection;

            // Apply end of chain processing and global output gain. A
            // numeric output gain is lifted into a _smoothControl so the
            // master fader can drive it live; the fader's source edits go
            // through $setMainGain. A signal-valued main gain renders as
            // a locked, live-tracking fader.
            this.processingEndOfChain = true;
            const mainGain = liftControl(
                this.mainGain,
                DEFAULT_OUTPUT_GAIN,
                OUT_GAIN_RANGE,
                '__vuGain_main',
            );
            const gainedMix = gainFactory(
                this.endOfChainCb(finalMix),
                mainGain.signal,
            );

            // The root output's lanes are device channels, so whatever stereo
            // the end-of-chain callback or a signal main gain brings in is
            // folded, keeping each lane on its own channel.
            signalFactory(monoFactory(gainedMix), { id: 'ROOT_OUTPUT' });

            // End-of-chain master meter, pinned to the panel's right side.
            // Taps ROOT_OUTPUT itself (post end-of-chain + global gain), so
            // it reads exactly what leaves the patch.
            vuMeters.push({
                baseChannel: 0,
                channels: 2,
                gain: mainGain.control,
                key: '__main__',
                label: 'Main',
                main: true,
                moduleId: 'ROOT_OUTPUT',
                mute: false,
                muteModuleId: undefined,
                ordinal: Number.MAX_SAFE_INTEGER,
                pan: null,
                portName: 'output',
                solo: false,
                sourceLocation: undefined,
            });
        } else {
            for (const group of this.groups) {
                group.finalize();
            }
            // No outputs registered - create silent root signal (0V)
            signalFactory(0, { id: 'ROOT_OUTPUT' });
            for (const bus of this.busses) {
                bus.finalize();
            }
        }

        // A tap control is the first lane of its source: tapped directly
        // when the source is ranged, as a lifted control is, and otherwise
        // through a `$signal` declaring the control's usual span.
        const control = (
            source: PolySignal,
            range: ControlRange,
        ): ModuleOutput => {
            // Every deferred is set by now, so a deferred source is judged by
            // the output it was set to.
            const lane = firstLane(source);
            const first =
                lane instanceof DeferredModuleOutput
                    ? (lane.resolve() ?? lane)
                    : lane;
            if (
                first instanceof ModuleOutput &&
                this.rangedPorts.has(`${first.moduleId}:${first.portName}`)
            ) {
                return first;
            }
            return (signalFactory(first, { ...range }) as Collection).items[0];
        };
        // Every tap of a track shares its sources.
        const outSources = new Map<string, TrackTapSources>();
        for (const { label, tap, sourceLocation } of this.trackTaps) {
            const out = outsByLabel.get(trackNameKey(label));
            if (!out) {
                throw new Error(
                    `$trackTap: no track is named "${label}"${lineSuffix(sourceLocation)}`,
                );
            }
            let sources = outSources.get(trackNameKey(label));
            if (!sources) {
                sources = {
                    active: control(out.active, GATE_RANGE),
                    audio: out.audio,
                    level: control(out.level, OUT_GAIN_RANGE),
                    panning: control(out.pan, OUT_PAN_RANGE),
                };
                outSources.set(trackNameKey(label), sources);
            }
            if (out.isBus) {
                // Every track sends to every bus, so a track reading a bus's
                // audio would feed back into it.
                tap.resolveControls(
                    sources,
                    `${label} is a bus, so its audio can't be used here${lineSuffix(sourceLocation)}. A bus's audio is processed only on its own track: chain the effect onto ${label}'s $bus() instead. Its name still works in sends, and .active, .level and .panning read its mixer controls.`,
                );
            } else {
                tap.resolve(sources);
            }
        }

        for (const meter of vuMeters) {
            const sources: ControlTap[] = [];
            for (const knob of [
                meter.gain,
                meter.pan,
                ...(meter.sends ?? []),
            ]) {
                const tap =
                    knob && this.resolveControlTap(controlTaps.get(knob));
                if (tap) {
                    knob.sourceIndex = sources.push(tap) - 1;
                }
            }
            if (sources.length > 0) {
                meter.controlSources = sources;
            }
        }

        // The VU panel lists meters in source order, not channel order.
        vuMeters.sort((a, b) => a.ordinal - b.ordinal);

        // Update ROOT_CLOCK tempo with the current tempo setting
        const rootClock = this.modules.get('ROOT_CLOCK');
        if (rootClock) {
            rootClock.params.tempo = this.tempo;
            rootClock.params.numerator = this.timeSignatureNumerator;
            rootClock.params.denominator = this.timeSignatureDenominator;
            rootClock.params.tempoSet = this.tempoExplicitlySet;
            if (this.groove !== undefined) {
                rootClock.params.groove = this.groove;
            }
        }

        const { eqPanels, eqScopes } = this.collectEqPanels();
        this.eqPanels = eqPanels;

        // Build a map of deferred output strings to their resolved output strings
        const deferredStringMap = new Map<string, string | null>();
        for (const deferred of this.deferredOutputs.values()) {
            const deferredStr = deferred.toString();
            const resolved = deferred.resolve();
            if (resolved) {
                deferredStringMap.set(deferredStr, resolved.toString());
            } else {
                deferredStringMap.set(deferredStr, null);
            }
        }

        const ret = {
            modules: Array.from(this.modules.values()).map((m) => {
                // First replace signals (ModuleOutput -> cable objects)
                const replacedParams = replaceDeferred(
                    replaceSignals(m.params),
                    this.deferredOutputs,
                );
                // Then replace any deferred strings with resolved strings
                const finalParams = replaceDeferredStrings(
                    replacedParams,
                    deferredStringMap,
                );
                const moduleSchema = this.schemaByName.get(m.moduleType);
                if (!moduleSchema) {
                    throw new Error(
                        `Schema for module "${m.moduleType}" not found.`,
                    );
                }

                // Finally resolve $gN signal groups into plain cartesian
                // arrays — the emitted graph carries no wrapper objects.
                return {
                    ...m,
                    params: expandSignalGroups(
                        finalParams as Record<string, unknown>,
                        moduleSchema,
                        m.moduleType,
                    ),
                };
            }),
            scopes: [
                ...this.scopes.map(
                    (scope) =>
                        ({
                            ...scope,
                            // A deferred lane resolves to its output only
                            // now, so a mono one's second trace drops here.
                            lanes: this.scopedLanes(scope)
                                .map((lane) => this.resolveScopeLane(lane))
                                .filter(
                                    (lane) =>
                                        lane.side === undefined ||
                                        this.portChannels(
                                            lane.moduleId,
                                            lane.portName,
                                        ) !== 'mono',
                                ),
                        }) as ScopeWithLocation,
                ),
                ...eqScopes,
            ],
            // Napi maps both null and undefined to None, so the renderer's
            // null-based VuMeterDef satisfies VuMeterSpec's optional fields.
            vuMeters: vuMeters as unknown as PatchGraph['vuMeters'],
        };

        const feedback = findBusFeedback(
            ret.modules,
            feedbackTracks,
            this.busses.flatMap((bus) => {
                const mixId = bus.mixModuleId();
                return bus.trackName !== undefined && mixId !== null
                    ? [{ mixId, name: bus.trackName }]
                    : [];
            }),
        );
        if (feedback) {
            throw new Error(
                `track ${feedback.track.label} uses the audio of bus ${feedback.bus}${lineSuffix(feedback.track.sourceLocation)}. A bus's audio is processed only on its own track: chain the effect onto ${feedback.bus}'s $bus() instead.`,
            );
        }

        if (
            process.env.MODULAR_DEBUG_LOG === '1' ||
            process.env.MODULAR_DEBUG_LOG === 'true'
        ) {
            console.log('Built PatchGraph:', ret);
        }
        return ret;
    }

    /**
     * One panel per `$eq` with `ui: true`, plus the headless scopes
     * feeding its spectrum. EQs sharing a call site (a loop or helper) keep
     * their zone placement but carry no editable sourceLocation.
     */
    private collectEqPanels(): {
        eqPanels: EqPanelDef[];
        eqScopes: ScopeWithLocation[];
    } {
        const eqModules = [...this.modules.values()].filter(
            (m) => m.moduleType === EQ_MODULE_TYPE && m.params.ui === true,
        );
        const siteKey = (id: string) => {
            const loc = this.sourceLocationMap.get(id);
            return loc ? `${loc.line}:${loc.column}` : null;
        };
        const perSite = new Map<string, number>();
        for (const m of eqModules) {
            const key = siteKey(m.id);
            if (key) {
                perSite.set(key, (perSite.get(key) ?? 0) + 1);
            }
        }

        const eqPanels: EqPanelDef[] = [];
        const eqScopes: ScopeWithLocation[] = [];
        for (const m of eqModules) {
            const loc = this.sourceLocationMap.get(m.id);
            const zoneLocation = loc
                ? { column: loc.column, line: loc.line }
                : undefined;
            const sections = m.params.sections;
            const lists =
                Array.isArray(sections) && Array.isArray(sections[0])
                    ? sections.length
                    : 1;
            const spectrum = {
                lanes: Array.from({ length: lists }, (_, i) => i),
                moduleId: m.id,
                msPerFrame: EQ_SPECTRUM_MS_PER_FRAME,
                portName: 'output',
            };
            eqPanels.push({
                key: `eq:${m.id}`,
                moduleId: m.id,
                sourceLocation:
                    zoneLocation && perSite.get(siteKey(m.id)!) === 1
                        ? zoneLocation
                        : undefined,
                spectrum,
                zoneLocation,
            });
            for (const lane of spectrum.lanes) {
                eqScopes.push({
                    lanes: [
                        {
                            lane,
                            moduleId: m.id,
                            portName: spectrum.portName,
                        },
                    ],
                    headless: true,
                    msPerFrame: spectrum.msPerFrame,
                    range: [-5, 5],
                    triggerThreshold: undefined,
                });
            }
        }
        return { eqPanels, eqScopes };
    }

    getEqPanels(): EqPanelDef[] {
        return this.eqPanels;
    }

    /**
     * Reset the builder state
     */
    reset(): void {
        this.modules.clear();
        this.rangedPorts.clear();
        this.scopes = [];
        this.scopeRoots.clear();
        this.eqPanels = [];
        this.counters.clear();
        this.outGroups.clear();
        this.bussesByName.clear();
        this.groups = [];
        this.groupsByName.clear();
        this.pendingSends = [];
        this.outGroupOrdinal = 0;
        this.sourceLocationMap.clear();
        this.deferredOutputs.clear();
        this.trackTaps = [];
        this.tempo = 120;
        this.mainGain = DEFAULT_OUTPUT_GAIN;
        this.timeSignatureNumerator = 4;
        this.timeSignatureDenominator = 4;
        this.groove = undefined;
    }

    /**
     * Get the source location map for error reporting.
     * Maps module IDs to their source locations in the DSL code.
     */
    getSourceLocationMap(): Map<string, SourceLocation> {
        return this.sourceLocationMap;
    }

    /**
     * Register the track a label declares, from the labelled value: a
     * signal or its `.track()`.
     */
    addTrack(
        declaration: TrackDeclaration,
        value: unknown,
        sourceLocation?: { line: number; column: number },
    ): void {
        if (this.processingEndOfChain) {
            throw new Error(
                'a track label is not allowed in the end of chain processor callback.',
            );
        }
        const { name, channel, mono } = declaration;
        const track = value instanceof Track ? value : undefined;
        const source = track ? track.source : value;
        const options = track?.options ?? {};
        if (
            mono &&
            (options.pan !== undefined || options.spread !== undefined)
        ) {
            throw new Error(
                `track ${name} is mono, so its .track() takes no pan or spread`,
            );
        }
        if (
            source === undefined ||
            source === null ||
            typeof source === 'boolean' ||
            (typeof source === 'object' && !(Symbol.iterator in source))
        ) {
            throw new Error(
                `track ${name} must be a signal, got ${source === null ? 'null' : typeof source}`,
            );
        }
        const outputs = this.$c(source as PolySignal).items;
        if (outputs.length === 0) {
            throw new Error(`track ${name} has no signal`);
        }
        const root = signalRootOf(source);
        const bus = root instanceof Bus ? root : undefined;
        const ownGroup = root instanceof Group ? root : undefined;
        if (ownGroup) {
            if (ownGroup.trackName !== undefined) {
                throw new Error(
                    `track ${name} plays the $group that track ${ownGroup.trackName} plays: a group has one track, the one label suffixes name`,
                );
            }
            ownGroup.trackName = name;
            this.groupsByName.set(name, ownGroup);
        }
        if (bus && declaration.group !== null) {
            throw new Error(
                `track ${name} is a bus, and a bus cannot play into a group`,
            );
        }
        if (bus) {
            if (declaration.state.solo) {
                throw new Error(
                    `track ${name} is a bus, and a bus cannot be soloed`,
                );
            }
            if (options.sends !== undefined) {
                throw new Error(
                    `track ${name} is a bus, and a bus cannot send to another bus`,
                );
            }
            if (bus.trackName !== undefined) {
                throw new Error(
                    `track ${name} plays the $bus that track ${bus.trackName} plays: a bus has one track, the one sends name`,
                );
            }
            bus.trackName = name;
            this.bussesByName.set(name, bus);
        } else if (options.sends !== undefined) {
            this.pendingSends.push({
                sends: options.sends,
                sourceLocation,
                track: name,
            });
        }
        const state = {
            displayName: declaration.displayName,
            fromTrack: track !== undefined,
            group: declaration.group,
            isGroup: ownGroup !== undefined,
            isBus: bus !== undefined,
            gain: options.gain,
            label: name,
            mute: declaration.state.mute,
            ordinal: this.outGroupOrdinal++,
            outputs,
            scopes: track?.scopes ?? [],
            solo: declaration.state.solo,
            sourceLocation,
        };
        const group: OutGroup = mono
            ? { ...state, type: 'mono' }
            : {
                  ...state,
                  pan: options.pan,
                  spread: options.spread,
                  type: 'stereo',
              };
        const existing = this.outGroups.get(channel) ?? [];
        existing.push(group);
        this.outGroups.set(channel, existing);
    }

    addGroup(group: Group) {
        if (this.processingEndOfChain) {
            throw new Error(
                '`$group` is not allowed in the end of chain processor callback.',
            );
        }
        this.groups.push(group);
    }

    addBus(bus: Bus) {
        if (this.processingEndOfChain) {
            throw new Error(
                '`$bus` is not allowed in the end of chain processor callback.',
            );
        }
        this.busses.push(bus);
    }

    /** The bus `value` names: a bus track's name. */
    private sendTarget(value: unknown): Bus | undefined {
        const name = trackBindingName(value);
        return name === undefined ? undefined : this.bussesByName.get(name);
    }

    /**
     * Whether `value` is one `[bus, level]` pair rather than a list of sends:
     * a bus followed by something that is neither a bus nor a pair.
     */
    private isSendPair(value: unknown): value is [unknown, PolySignal] {
        if (!Array.isArray(value) || value.length !== 2) {
            return false;
        }
        const [bus, level] = value as unknown[];
        // A track's name second is a second send, never a level, so a
        // name that is no bus is reported instead of becoming a level.
        return (
            this.sendTarget(bus) !== undefined &&
            trackBindingName(level) === undefined &&
            !(Array.isArray(level) && trackBindingName(level[0]) !== undefined)
        );
    }

    /** The `[bus, level]` pairs of `track`'s `sends` option. */
    private resolveSends(
        track: string,
        sends: unknown,
        sourceLocation?: { line: number; column: number },
    ): [Bus, PolySignal | undefined][] {
        const entries =
            this.sendTarget(sends) !== undefined || this.isSendPair(sends)
                ? [sends]
                : sends;
        const invalid = (entry: unknown): Error => {
            const target = trackBindingName(
                Array.isArray(entry) ? entry[0] : entry,
            );
            return new Error(
                target === undefined
                    ? `track ${track}: sends takes a bus track's name, a [name, level] pair, or an array of them${lineSuffix(sourceLocation)}`
                    : `track ${track}: sends cannot reach ${target}, which is not a bus${lineSuffix(sourceLocation)}. A bus is a track whose signal is $bus() or methods chained on it; to set a send's level from a track, wrap its name, as in [bus, $c(name)]`,
            );
        };
        if (!Array.isArray(entries)) {
            throw invalid(entries);
        }
        return entries.map((entry: unknown) => {
            const bus = this.sendTarget(entry);
            if (bus) {
                return [bus, undefined];
            }
            if (this.isSendPair(entry)) {
                return [this.sendTarget(entry[0])!, entry[1]];
            }
            throw invalid(entry);
        });
    }

    /**
     * Register an `$trackTap` tap. It resolves in toPatch, so the track may
     * be labelled after the tap; a name no track has throws there.
     */
    addTrackTap(
        label: string,
        tap: PendingTrackTap,
        sourceLocation?: { line: number; column: number },
    ): void {
        this.trackTaps.push({ label, sourceLocation, tap });
    }

    /**
     * Register a deferred output for tracking.
     * Called by DeferredModuleOutput constructor.
     */
    registerDeferred(deferred: DeferredModuleOutput): void {
        this.deferredOutputs.set(deferred.moduleId, deferred);
    }

    addScope(
        value: ModuleOutput | ModuleOutput[],
        config: ScopeConfig = {},
        sourceLocation?: { line: number; column: number },
    ) {
        const {
            msPerFrame = 500,
            triggerThreshold,
            range = [-5, 5],
            split,
        } = config;
        if (
            split !== undefined &&
            split !== 'vertical' &&
            split !== 'horizontal'
        ) {
            throw new Error(
                `scope split must be 'vertical' or 'horizontal', got ${JSON.stringify(split)}`,
            );
        }
        const realTriggerThreshold: number | undefined =
            triggerThreshold !== undefined
                ? triggerThreshold * 1000
                : undefined;
        const triggerWaitToRender = config.triggerWaitToRender ?? true;
        let thresh: [number, ScopeMode] | undefined = undefined;
        if (realTriggerThreshold !== undefined) {
            thresh = [
                realTriggerThreshold,
                triggerWaitToRender ? 'Wait' : 'Roll',
            ];
        }

        const scope: ScopeWithLocation = {
            lanes: this.scopeLanes(Array.isArray(value) ? value : [value]),
            msPerFrame,
            range,
            sourceLocation,
            split,
            triggerThreshold: thresh,
        };
        this.scopes.push(scope);
        return scope;
    }

    /**
     * Scope a multi-output root as the output it is first plugged in as:
     * `$perc($('0').scope())` draws the trigger `$perc` takes. A root never
     * plugged in draws the output it reads as itself.
     */
    scopeRoot(
        set: OutputSet,
        value: ModuleOutput[],
        config: ScopeConfig | undefined,
        sourceLocation?: { line: number; column: number },
    ) {
        this.scopeRoots.set(this.addScope(value, config, sourceLocation), set);
    }

    /** The lanes `scope` draws, a scoped root's those of the output it was taken as. */
    private scopedLanes(scope: ScopeWithLocation): ScopeWithLocation['lanes'] {
        const set = this.scopeRoots.get(scope);
        const name = set && this.outputResolver.firstChoice(set);
        return name
            ? this.scopeLanes((set.values[name] as AnyCollection).items)
            : scope.lanes;
    }

    /**
     * A lane that may be stereo draws both sides; a mono one reads the same
     * on either, so its second trace would only repeat the first.
     */
    private scopeLanes(outputs: ModuleOutput[]): ScopeWithLocation['lanes'] {
        return outputs.flatMap((o) => {
            const lane = {
                lane: o.lane,
                moduleId: o.moduleId,
                portName: o.portName,
            };
            return this.channelsOf(o) === 'mono'
                ? [lane]
                : [lane, { ...lane, side: 1 }];
        });
    }

    /**
     * Resolve a VU control tap naming a deferred output to the output it was
     * set to. An unset deferred has nothing to sample, so it gets no tap.
     */
    private resolveControlTap(
        tap: ControlTap | undefined,
    ): ControlTap | undefined {
        const deferred = tap && this.deferredOutputs.get(tap.moduleId);
        if (!deferred) {
            return tap;
        }
        const resolved = deferred.resolve();
        return resolved
            ? {
                  lane: resolved.lane ?? 0,
                  moduleId: resolved.moduleId,
                  portName: resolved.portName,
              }
            : undefined;
    }

    /**
     * Resolve a scope lane ref against the $deferred outputs: a lane
     * naming a deferred module is replaced by the module it was set to.
     * Throws if the $deferred was never set.
     */
    private resolveScopeLane(lane: ScopeLane): ScopeLane {
        const deferred = this.deferredOutputs.get(lane.moduleId);
        if (!deferred) return lane;
        const resolved = deferred.resolve();
        if (resolved === null) {
            throw new Error(
                deferred.unsetReason ??
                    'Unset DeferredModuleOutput used in a scope — call .set(...) on the $deferred() before the end of the script',
            );
        }
        return {
            lane: lane.lane,
            moduleId: resolved.moduleId,
            portName: resolved.portName,
            side: lane.side,
        };
    }
}

/**
 * ModuleNode represents a module instance in the DSL (internal use only).
 * DSL code sees only the collections of its outputs.
 */
export class ModuleNode {
    readonly builder: GraphBuilder;
    readonly id: string;
    readonly moduleType: string;
    readonly schema: ProcessedModuleSchema;
    private _laneCount: number = 1;

    constructor(
        builder: GraphBuilder,
        id: string,
        moduleType: string,
        schema: ProcessedModuleSchema,
    ) {
        this.builder = builder;
        this.id = id;
        this.moduleType = moduleType;
        this.schema = schema;
    }

    /**
     * Get the number of lanes this module produces.
     * Set by Rust-side derivation via _setDerivedLaneCount.
     */
    get laneCount(): number {
        return this._laneCount;
    }

    _setParam(paramName: string, value: unknown): this {
        this.builder.setParam(this.id, paramName, replaceSignals(value));
        return this;
    }

    /**
     * Get a snapshot of the current params for this module.
     * Used for Rust-side lane count derivation. Signal groups are
     * expanded here so the engine only ever sees plain signal arrays.
     */
    getParamsSnapshot(): Record<string, unknown> {
        const raw = this.builder.getModule(this.id)?.params ?? {};
        return expandSignalGroups(raw, this.schema, this.moduleType);
    }

    /**
     * Set the lane count derived from Rust-side analysis.
     */
    _setDerivedLaneCount(lanes: number): void {
        this._laneCount = lanes;
    }

    /**
     * Whether the engine knows the range of `output`, as far as the DSL can
     * tell without running the patch: a fixed range, one the module's own
     * params set or declare, or one that follows inputs that are ranged or
     * constant, at least one ranged, unless a bounding param rules it out.
     * It errs towards unranged, so a ranged output is always bounded in the
     * engine.
     */
    private isRanged(output: OutputSchema): boolean {
        switch (output.bounds as BoundsKind) {
            case 'fixed':
            case 'params':
                return true;
            case 'input': {
                if (this.builder.declaresRange(this.id)) {
                    return true;
                }
                const params = this.builder.getModule(this.id)?.params ?? {};
                // A bounding param set to another value rules a range out.
                const bounding = this.schema.boundingParam;
                const decider = bounding ? params[bounding.name] : undefined;
                if (
                    bounding &&
                    typeof decider === 'string' &&
                    !bounding.values.includes(decider)
                ) {
                    return false;
                }
                // A constant holds still: it carries the range along but has
                // none to give, so some input must be ranged in every lane.
                const tagged = this.schema.signalParams
                    .filter((param) => param.carriesBounds)
                    .map((param) => ({
                        kind: this.builder.rangeOf(params[param.name]),
                        needsConstant: param.boundsNeedConstant,
                    }));
                return (
                    tagged.every(({ kind, needsConstant }) =>
                        needsConstant
                            ? kind === 'constant'
                            : kind !== 'unranged',
                    ) && tagged.some(({ kind }) => kind === 'ranged')
                );
            }
            default:
                return false;
        }
    }

    /**
     * An output port of this module, as a collection of its lanes: ranged
     * when the engine knows its range. A mono port is one lane.
     */
    _output(portName: string): Collection | CollectionWithRange {
        const outputSchema = this.schema.outputs.find(
            (o) => o.name === portName,
        );
        if (!outputSchema) {
            throw new Error(
                `Module ${this.moduleType} does not have output: ${portName}`,
            );
        }

        const lanes = outputSchema.polyphonic ? this.laneCount : 1;
        const items = Array.from(
            { length: lanes },
            (_, i) => new ModuleOutput(this.builder, this.id, portName, i),
        );
        if (this.isRanged(outputSchema)) {
            this.builder.markRanged(this.id, portName);
            return new CollectionWithRange(...items);
        }
        return new Collection(...items);
    }
}

/**
 * One lane of a module output port. Internal plumbing: the DSL hands out
 * collections of these, never a bare lane.
 */
export class ModuleOutput {
    readonly builder: GraphBuilder;
    readonly moduleId: string;
    readonly portName: string;
    readonly lane: number = 0;

    constructor(
        builder: GraphBuilder,
        moduleId: string,
        portName: string,
        lane: number = 0,
    ) {
        this.builder = builder;
        this.moduleId = moduleId;
        this.portName = portName;
        this.lane = lane;
    }

    toString(): string {
        return `module(${this.moduleId}:${this.portName}:${this.lane})`;
    }
}

/**
 * DeferredModuleOutput is a lane placeholder for a signal assigned later,
 * for feedback loops and forward references. Cables to it resolve at the end
 * of the script to the lane it was set to.
 */
export class DeferredModuleOutput extends ModuleOutput {
    private resolvedModuleOutput: ModuleOutput | null = null;
    private resolving: boolean = false;
    static idCounter = 0;
    /** Why this output is never set, reported where the patch reads it. */
    unsetReason: string | null = null;

    constructor(builder: GraphBuilder) {
        super(
            builder,
            `DEFERRED-${DeferredModuleOutput.idCounter++}`,
            'output',
        );
        // Register this deferred output with the builder for string replacement during toPatch
        builder.registerDeferred(this);
    }

    /**
     * Set the lane this placeholder resolves to. Bare Signal literals
     * (numbers, note/Hz strings) are lifted into $signal modules, matching
     * `$c`.
     */
    set(signal: Signal): void {
        if (typeof signal === 'number' || typeof signal === 'string') {
            this.resolvedModuleOutput = this.builder.$c(signal).items[0];
            return;
        }
        this.resolvedModuleOutput = signal;
    }

    /**
     * Resolve this deferred output to an actual ModuleOutput.
     * @returns The resolved ModuleOutput, or null if not set.
     */
    resolve(): ModuleOutput | null {
        if (this.resolving) {
            throw new Error(
                'Circular reference detected while resolving DeferredModuleOutput',
            );
        }

        if (this.resolvedModuleOutput === null) {
            return null;
        }

        let output = this.resolvedModuleOutput;
        if (output instanceof DeferredModuleOutput) {
            this.resolving = true;
            const resolved = output.resolve();
            this.resolving = false;

            if (resolved === null) {
                return null;
            }
            output = resolved;
        }

        return output;
    }
}

/**
 * DeferredCollection is a collection of DeferredModuleOutput instances.
 * Provides a .set() method to assign signals to all contained deferred outputs.
 */
export class DeferredCollection extends BaseCollection<DeferredModuleOutput> {
    /**
     * Set the signals for all deferred outputs in this collection, cycling a
     * narrower argument across the lanes. Bare Signal literals (numbers,
     * note/Hz strings) are lifted into $signal modules, matching `$c` — a
     * string is one signal, not spread into characters.
     * @param outputs - A Poly<Signal> (single signal, array, or iterable) to distribute across outputs
     */
    set(outputs: PolySignal): void {
        if (this.items.length === 0) {
            return;
        }
        const outputsArr = this.items[0].builder.$c(outputs).items;
        if (outputsArr.length === 0) {
            throw new Error(
                'DeferredCollection.set() requires at least one signal',
            );
        }

        // Distribute signals across deferred outputs
        for (let i = 0; i < this.items.length; i++) {
            this.items[i].set(outputsArr[i % outputsArr.length]);
        }
    }
}

/**
 * Crossfade `original` (dry) against `result` (wet) by `mix` (0 = dry, 5 = wet,
 * 2.5 = equal) through `$crossfade`, which cycles the narrower leg across the
 * wider. Backs `.pipeMix` and `.wet`.
 */
function crossfadeMix(
    builder: GraphBuilder,
    original: AnyCollection,
    result: AnyCollection,
    mix: PolySignal = 2.5,
): Collection {
    const amount = builder.outputResolver.resolve(
        mix,
        'cv',
        captureSourceLocation(),
        MIX_SLOT,
    ) as PolySignal;
    return builder.getFactory('$crossfade')(
        sumsLanes(builder, result) && original.items.length > 1
            ? builder.getFactory('$mix')(
                  original.items.map((item) => new Collection(item)),
              )
            : original,
        result,
        amount,
    ) as Collection;
}

/**
 * Whether `result` comes from a stereo effect, which sums every lane of its
 * input into one stereo lane. The dry voices are then summed the same way,
 * so the crossfade hears the wet once rather than once per dry voice.
 */
function sumsLanes(builder: GraphBuilder, result: AnyCollection): boolean {
    const first = result.items[0];
    if (!(first instanceof ModuleOutput) || result.items.length !== 1) {
        return false;
    }
    const schema = builder.schemaOf(first.moduleId);
    return schema?.stereo === 'stereo' && schema.lanes === 1;
}

/** `receiver.wet(mix)`: fade the signal `receiver` was chained from against it. */
function wetMix(
    builder: GraphBuilder,
    receiver: AnyCollection,
    mix: PolySignal,
): Collection {
    const dry = drySourceOf(receiver);
    if (dry === undefined) {
        throw new Error(
            ".wet() follows a chained module, as in sig.lpf('800hz').wet(2.5)",
        );
    }
    return crossfadeMix(builder, dry as AnyCollection, receiver, mix);
}

const chains = createChainInstaller({
    empty: () => new Collection(),
    hostOf: (receiver) => (receiver as AnyCollection).items[0]?.builder,
    prototypes: [BaseCollection.prototype],
    subclasses: [
        Collection.prototype,
        CollectionWithRange.prototype,
        DeferredCollection.prototype,
    ],
});

type Replacer = (key: string, value: unknown) => unknown;

export function replaceValues(input: unknown, replacer: Replacer): unknown {
    function walk(key: string, value: unknown): unknown {
        const replaced = replacer(key, value);

        // Match JSON.stringify behavior
        if (replaced === undefined) {
            return undefined;
        }

        if (typeof replaced !== 'object' || replaced === null) {
            return replaced;
        }

        // A pattern payload is returned verbatim, with the payloads nested in
        // it: walking it would collapse the nulls in its `accidental`,
        // `octave` and weight slots to 0 via valueToSignal, producing
        // zero-duration haps and silence. A SignalGroup wrapper is walked, so
        // its outputs become cables; expandSignalGroups later erases it.
        if (isPatternPayload(replaced)) {
            return replaced;
        }

        if (Array.isArray(replaced)) {
            return replaced
                .map((v, i) => walk(String(i), v))
                .filter((v) => v !== undefined);
        }

        const out: Record<string, unknown> = {};
        for (const [k, v] of Object.entries(replaced)) {
            const entryVal = walk(k, v);
            if (entryVal !== undefined) {
                out[k] = entryVal;
            }
        }
        return out;
    }

    // JSON.stringify starts with key ""
    return walk('', input);
}

/**
 * A collection as a param value: the array of its lanes. Nested in an
 * array or object (a tuple element, a nested field), a one-lane collection
 * is that lane, so it fills a single-signal slot.
 */
function collectionValue(
    key: string,
    value: AnyCollection,
    replaceLane: (lane: ModuleOutput) => unknown,
): unknown {
    return key !== '' && value.items.length === 1
        ? replaceLane(value.items[0])
        : value.items;
}

export function replaceSignals(input: unknown): unknown {
    return replaceValues(input, (key, value) => {
        if (value instanceof BaseCollection) {
            return collectionValue(key, value, valueToSignal);
        }

        return valueToSignal(value);
    });
}

/**
 * Recursively replace deferred output strings with resolved output strings in params.
 * This handles cases where a DeferredModuleOutput was stringified (e.g., in pattern strings).
 */
export function replaceDeferredStrings(
    input: unknown,
    deferredStringMap: Map<string, string | null>,
): unknown {
    if (typeof input === 'string') {
        // Replace all occurrences of deferred strings with resolved strings
        let result = input;
        for (const [deferredStr, resolvedStr] of deferredStringMap) {
            const splitResult = result.split(deferredStr);
            if (splitResult.length > 1) {
                if (resolvedStr === null) {
                    throw new Error(
                        `Unset DeferredModuleOutput used in string: "${input}"`,
                    );
                }

                result = splitResult.join(resolvedStr);
            }
        }
        return result;
    }

    if (Array.isArray(input)) {
        return input.map((item) =>
            replaceDeferredStrings(item, deferredStringMap),
        );
    }

    if (typeof input === 'object' && input !== null) {
        // A pattern payload holds no deferred-output strings and is returned
        // verbatim, as in replaceValues. A SignalGroup wrapper is walked: its
        // signals may carry deferred-output strings to resolve.
        if (isPatternPayload(input)) {
            return input;
        }

        const result: Record<string, unknown> = {};
        for (const [key, value] of Object.entries(input)) {
            result[key] = replaceDeferredStrings(value, deferredStringMap);
        }
        return result;
    }

    return input;
}

function replaceDeferred(
    input: unknown,
    deferredOutputs: Map<string, DeferredModuleOutput>,
): unknown {
    function replace(value: unknown): unknown {
        const maybeResolvedModuleOutput = ResolvedModuleOutput.safeParse(value);
        if (maybeResolvedModuleOutput.success) {
            const resolved = deferredOutputs.get(
                maybeResolvedModuleOutput.data.module,
            );
            if (resolved) {
                const output = resolved.resolve();
                if (output === null) {
                    throw new Error(
                        resolved.unsetReason ??
                            'Unset DeferredModuleOutput used as a module param — call .set(...) on the $deferred() before the end of the script',
                    );
                }
                return valueToSignal(output);
            }
            return maybeResolvedModuleOutput.data;
        }
        return value;
    }
    return replaceValues(input, (key, value) => {
        if (value instanceof BaseCollection) {
            return collectionValue(key, value, (lane) =>
                replace(valueToSignal(lane)),
            );
        }

        return replace(value);
    });
}

/**
 * First lane of a signal param value: the value itself for a scalar or
 * ModuleOutput, else the first element of an array, Collection, or `$gN`
 * group. An empty value reads as 0.
 */
function firstLane(value: unknown): Signal {
    if (isSignalGroupLike(value)) {
        return firstLane(value.signals);
    }
    if (
        value instanceof ModuleOutput ||
        typeof value === 'number' ||
        typeof value === 'string'
    ) {
        return value;
    }
    if (value instanceof BaseCollection || Array.isArray(value)) {
        const [first] = toSignalArray(value as PolySignal);
        return first === undefined ? 0 : firstLane(first);
    }
    return 0;
}

function valueToSignal(value: unknown): unknown {
    if (value instanceof ModuleOutput) {
        return {
            lane: value.lane,
            module: value.moduleId,
            port: value.portName,
            type: 'cable',
        };
    } else if (value === null || value === undefined) {
        // Silence: 0 becomes Signal::Volts(0.0) in Rust
        return 0;
    }
    // It's a number
    return value;
}
