import { captureSourceLocation } from './captureSourceLocation';
import {
    asRanged,
    Collection,
    type CollectionWithRange,
    DeferredCollection,
    DeferredModuleOutput,
    type GraphBuilder,
    type ModuleOutput,
} from './GraphBuilder';

/**
 * A labeled out lane's signals, as compiled in toPatch. The controls are
 * outputs the engine knows the range of.
 */
export interface OutLaneSources {
    /** Post-gain, post-mute audio: two channels for stereo outs, one for mono. */
    audio: ModuleOutput[];
    /** Mute/solo gate (5 audible, 0 silenced). */
    active: ModuleOutput;
    level: ModuleOutput;
    /** Center (0) for mono outs. */
    pan: ModuleOutput;
}

interface OutLaneControls {
    active: CollectionWithRange;
    level: CollectionWithRange;
    pan: CollectionWithRange;
}

/**
 * `$outLane` result: a stereo Collection of the lane's audio, with its
 * mute/solo gate, gain, and pan as ranged properties. Slices keep the
 * properties.
 */
export class OutLane extends Collection {
    /** 5 V while the lane is audible, 0 V while muted or soloed out. */
    readonly active: CollectionWithRange;
    /** The lane's gain in DSL units (5 = unity). */
    readonly level: CollectionWithRange;
    /** The lane's pan (-5 left … +5 right). */
    readonly pan: CollectionWithRange;

    constructor(audio: ModuleOutput[], controls: OutLaneControls) {
        super(...audio);
        this.active = controls.active;
        this.level = controls.level;
        this.pan = controls.pan;
    }

    override slice(start?: number, end?: number): this {
        return new OutLane(this.items.slice(start, end), this) as this;
    }
}

/**
 * The deferred outputs behind one `$outLane` call. Every channel and control
 * is deferred because the out may be declared after the tap; the audio is
 * fixed at two channels so consumers can derive channel counts at call time.
 * The controls are ranged: each resolves to an output whose range the engine
 * knows.
 */
export class OutLaneTap {
    readonly lane: OutLane;
    private readonly audio: DeferredCollection;
    private readonly active: DeferredModuleOutput;
    private readonly level: DeferredModuleOutput;
    private readonly pan: DeferredModuleOutput;

    constructor(builder: GraphBuilder) {
        const deferred = () => new DeferredModuleOutput(builder);
        this.audio = new DeferredCollection(deferred(), deferred());
        this.active = deferred();
        this.level = deferred();
        this.pan = deferred();
        this.lane = new OutLane(this.audio.items, {
            active: asRanged(new Collection(this.active)),
            level: asRanged(new Collection(this.level)),
            pan: asRanged(new Collection(this.pan)),
        });
    }

    /** Point the lane at its out's signals; mono audio fills both channels. */
    resolve(sources: OutLaneSources): void {
        this.audio.set(sources.audio);
        this.active.set(sources.active);
        this.level.set(sources.level);
        this.pan.set(sources.pan);
    }
}

/** `$outLane(label)`: tap the out with that label. */
export function createOutLane(builder: GraphBuilder, label: string): OutLane {
    if (typeof label !== 'string' || label.length === 0) {
        throw new Error('$outLane: label must be a non-empty string');
    }
    const tap = new OutLaneTap(builder);
    builder.addOutLaneTap(label, tap, captureSourceLocation());
    return tap.lane;
}
