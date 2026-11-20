/** A lane the engine samples so a locked control can track its signal. */
export interface ControlTap {
    moduleId: string;
    portName: string;
    lane: number;
}

/**
 * A mixer control on a track: its fader, pan knob or a send dial. A number
 * within the control's range is lifted into a `_smoothControl` the panel
 * drives live; anything else stays wired as written, and the control locks.
 */
export interface VuControl {
    /** The value the control shows; null when a signal drives it. */
    value: number | null;
    /** `_smoothControl` the panel drives; null when locked. */
    moduleId: string | null;
    /** True when the control takes no input: a signal drives it, its
     *  number lies past the control's range, or a send names its bus twice. */
    locked: boolean;
    /** Index of its live tap in the meter's `controlSources`, for a control a
     *  signal drives. */
    sourceIndex?: number;
}

/** A track's send to one bus, as the mixer's send dial shows it. Its value
 *  is a linear level: 5 for a bus named alone, 0 where none is named. */
export interface VuSendDef extends VuControl {
    /** The bus's track name: the key of its meter. */
    bus: string;
}

/**
 * One track's VU meter entry, emitted by the GraphBuilder on
 * `PatchGraph.vuMeters`. Rust reads the metering fields (moduleId / portName /
 * channels / muteModuleId); the rest is renderer metadata that rides through
 * `appliedPatch` untouched (same contract as `Scope.sourceLocation`).
 */
export interface VuMeterDef {
    /**
     * Stable identity: the track's name, which its mute/solo prefix and
     * channel suffix leave out. Unique across every meter, including the
     * master's reserved `__main__`.
     */
    key: string;
    label: string | null;
    /** Module whose output port is metered (the post-gain, pre-mute tap). */
    moduleId: string;
    portName: string;
    /** 1 (mono) or 2 (stereo). */
    channels: number;
    /**
     * `$signal` driving the mute gate: value 5 = audible, 0 = silenced.
     * Absent for the end-of-chain master meter, which has no mute/solo
     * (undefined rather than null: napi's Option only accepts undefined).
     */
    muteModuleId?: string;
    baseChannel: number;
    mute: boolean;
    solo: boolean;
    /** Pan (-5 left … +5 right); null for a mono track, which has no knob. */
    pan: VuControl | null;
    /** Output gain in DSL units (audio taper; 5 = unity). */
    gain: VuControl;
    /**
     * One send per bus for a track or group, in no particular order; absent
     * on buses and the main strip. A track gets a send to every bus, at 0
     * when its source names none, so a dial is live from its first move.
     */
    sends?: VuSendDef[];
    /** Live taps of the locked controls, sampled into the meter frames'
     *  `controls`. */
    controlSources?: ControlTap[];
    /** True for the end-of-chain master meter pinned to the panel's right. */
    main?: boolean;
    /** The group track this track plays into, by key; null for none. */
    group?: string | null;
    /** Whether this track is a group's, which other tracks can play into. */
    isGroup?: boolean;
    /** Whether this track is a bus's, which the mixer shows beside the main. */
    isBus?: boolean;
    /**
     * Whether the track's label labels a `.track(...)` made elsewhere (in a
     * helper or a const), whose options the mixer cannot rewrite.
     */
    fromTrack?: boolean;
    /**
     * Location of the track's label in the DSL source; absent for the
     * master meter.
     */
    sourceLocation?: { line: number; column: number };
}

/**
 * Code-side control values that diverge from the running audio after a
 * Ctrl/Cmd (code-only) panel gesture. Renderer-only: a missing property
 * means code and audio agree for that control. Cleared per control when a
 * plain gesture writes both sides, and wholesale when a patch update
 * applies (the compiled source re-syncs audio to code).
 */
export interface VuMeterGhost {
    mute?: boolean;
    solo?: boolean;
    /** Pan position in the source, -5…+5. */
    pan?: number;
    /** Out gain in the source (DSL units, 0…10). */
    gain?: number;
    /** A send's level in the source, by its bus's key; 0 where the source
     *  names none. */
    [send: SendGhostKey]: number | undefined;
}

/** The ghost of the send to bus `bus`. */
export type SendGhostKey = `send:${string}`;

export const sendGhostKey = (bus: string): SendGhostKey => `send:${bus}`;

/** Out gain value producing unity amplitude. */
export const UNITY_OUT_GAIN = 5;

/** Global master output-gain default (unity), restored by a master-fader reset. */
export const DEFAULT_OUTPUT_GAIN = 5;

/**
 * The `$gain` curve that outs apply their gain through, mirroring
 * `audio_taper` in Rust: 12 dB per unit above the knee, then a straight line
 * to silence that meets the exponential tangentially.
 */
const GAIN_DB_PER_LEVEL = 12;
const GAIN_LN_PER_LEVEL = (GAIN_DB_PER_LEVEL / 20) * Math.LN10;
const GAIN_KNEE = 1 / GAIN_LN_PER_LEVEL;
const GAIN_KNEE_SLOPE = Math.E * 1e-3 * GAIN_LN_PER_LEVEL;
const GAIN_KNEE_DB = GAIN_DB_PER_LEVEL * (GAIN_KNEE - UNITY_OUT_GAIN);

/** Out gain (DSL units, 0…10) → dB. Returns -Infinity at 0. */
export function outGainToDb(gain: number): number {
    if (gain <= 0) {
        return -Infinity;
    }
    if (gain >= GAIN_KNEE) {
        return GAIN_DB_PER_LEVEL * (gain - UNITY_OUT_GAIN);
    }
    return 20 * Math.log10(gain * GAIN_KNEE_SLOPE);
}

/** dB → out gain (DSL units). -Infinity maps to 0. */
export function dbToOutGain(db: number): number {
    if (!Number.isFinite(db)) {
        return 0;
    }
    if (db >= GAIN_KNEE_DB) {
        return UNITY_OUT_GAIN + db / GAIN_DB_PER_LEVEL;
    }
    return Math.pow(10, db / 20) / GAIN_KNEE_SLOPE;
}
