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
 * A track's signals, as compiled in toPatch. The controls are
 * outputs the engine knows the range of.
 */
export interface TrackTapSources {
    /** Post-gain, post-mute audio: one lane, stereo for a stereo track. */
    audio: ModuleOutput[];
    /** Mute/solo gate (5 audible, 0 silenced). */
    active: ModuleOutput;
    level: ModuleOutput;
    /** Center (0) for mono tracks. */
    panning: ModuleOutput;
}

interface TrackTapControls {
    active: CollectionWithRange;
    level: CollectionWithRange;
    panning: CollectionWithRange;
}

/**
 * `$trackTap` result: a Collection of the track's audio, one lane, with its
 * mute/solo gate, gain, and panning as ranged properties. Slices keep the
 * properties.
 */
export class TrackTap extends Collection {
    /** 5 V while the track is audible, 0 V while muted or soloed out. */
    readonly active: CollectionWithRange;
    /** The track's gain in DSL units (5 = unity). */
    readonly level: CollectionWithRange;
    /** The track's pan (-5 left … +5 right). */
    readonly panning: CollectionWithRange;

    constructor(audio: ModuleOutput[], controls: TrackTapControls) {
        super(...audio);
        this.active = controls.active;
        this.level = controls.level;
        this.panning = controls.panning;
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
            panning: this.panning.concat(other.panning),
        }) as this;
    }
}

/**
 * The deferred outputs behind one `$trackTap` call. Every lane and control
 * is deferred because the track may be labelled after the tap; the audio is
 * fixed at one lane so consumers can derive lane counts at call time.
 * The controls are ranged: each resolves to an output whose range the engine
 * knows.
 */
export class PendingTrackTap {
    readonly tap: TrackTap;
    private readonly audio: DeferredCollection;
    private readonly active: DeferredModuleOutput;
    private readonly level: DeferredModuleOutput;
    private readonly panning: DeferredModuleOutput;

    constructor(builder: GraphBuilder) {
        const deferred = () => new DeferredModuleOutput(builder);
        this.audio = new DeferredCollection(deferred());
        this.active = deferred();
        this.level = deferred();
        this.panning = deferred();
        this.tap = new TrackTap(this.audio.items, {
            active: asRanged(new Collection(this.active)),
            level: asRanged(new Collection(this.level)),
            panning: asRanged(new Collection(this.panning)),
        });
    }

    /** Point the tap at its track's signals. */
    resolve(sources: TrackTapSources): void {
        this.audio.set(sources.audio);
        this.resolveControls(sources, null);
    }

    /**
     * Point the tap at its track's controls only; reading its audio fails
     * with `audioError`.
     */
    resolveControls(sources: TrackTapSources, audioError: string | null): void {
        for (const lane of this.audio.items) {
            lane.unsetReason = audioError;
        }
        this.active.set(sources.active);
        this.level.set(sources.level);
        this.panning.set(sources.panning);
    }
}

/** `$trackTap(name)`: tap the track with that name. */
export function createTrackTap(builder: GraphBuilder, label: string): TrackTap {
    if (typeof label !== 'string' || label.length === 0) {
        throw new Error('$trackTap: name must be a non-empty string');
    }
    const pending = new PendingTrackTap(builder);
    builder.addTrackTap(label, pending, captureSourceLocation());
    return pending.tap;
}
