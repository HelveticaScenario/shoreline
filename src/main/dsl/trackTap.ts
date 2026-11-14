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
 * A labeled out's signals, as compiled in toPatch. The controls are
 * outputs the engine knows the range of.
 */
export interface TrackTapSources {
    /** Post-gain, post-mute audio: two lanes for stereo outs, one for mono. */
    audio: ModuleOutput[];
    /** Mute/solo gate (5 audible, 0 silenced). */
    active: ModuleOutput;
    level: ModuleOutput;
    /** Center (0) for mono outs. */
    pan: ModuleOutput;
}

interface TrackTapControls {
    active: CollectionWithRange;
    level: CollectionWithRange;
    pan: CollectionWithRange;
}

/**
 * `$trackTap` result: a stereo Collection of the out's audio, with its
 * mute/solo gate, gain, and pan as ranged properties. Slices keep the
 * properties.
 */
export class TrackTap extends Collection {
    /** 5 V while the out is audible, 0 V while muted or soloed out. */
    readonly active: CollectionWithRange;
    /** The out's gain in DSL units (5 = unity). */
    readonly level: CollectionWithRange;
    /** The out's pan (-5 left … +5 right). */
    readonly pan: CollectionWithRange;

    constructor(audio: ModuleOutput[], controls: TrackTapControls) {
        super(...audio);
        this.active = controls.active;
        this.level = controls.level;
        this.pan = controls.pan;
    }

    override slice(start?: number, end?: number): this {
        return new TrackTap(this.items.slice(start, end), this) as this;
    }

    override concat(other: this): this {
        if (!(other instanceof TrackTap)) {
            throw new Error('concat() on a $trackTap takes another $trackTap');
        }
        return new TrackTap([...this.items, ...other.items], {
            active: this.active.concat(other.active),
            level: this.level.concat(other.level),
            pan: this.pan.concat(other.pan),
        }) as this;
    }
}

/**
 * The deferred outputs behind one `$trackTap` call. Every lane and control
 * is deferred because the out may be declared after the tap; the audio is
 * fixed at two lanes so consumers can derive lane counts at call time.
 * The controls are ranged: each resolves to an output whose range the engine
 * knows.
 */
export class PendingTrackTap {
    readonly tap: TrackTap;
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
        this.tap = new TrackTap(this.audio.items, {
            active: asRanged(new Collection(this.active)),
            level: asRanged(new Collection(this.level)),
            pan: asRanged(new Collection(this.pan)),
        });
    }

    /** Point the tap at its out's signals; mono audio fills both lanes. */
    resolve(sources: TrackTapSources): void {
        this.audio.set(sources.audio);
        this.active.set(sources.active);
        this.level.set(sources.level);
        this.pan.set(sources.pan);
    }
}

/** `$trackTap(label)`: tap the out with that label. */
export function createTrackTap(builder: GraphBuilder, label: string): TrackTap {
    if (typeof label !== 'string' || label.length === 0) {
        throw new Error('$trackTap: label must be a non-empty string');
    }
    const pending = new PendingTrackTap(builder);
    builder.addTrackTap(label, pending, captureSourceLocation());
    return pending.tap;
}
