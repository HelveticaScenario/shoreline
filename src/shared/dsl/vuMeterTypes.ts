/** A lane the engine samples so a locked control can track its signal. */
export interface ControlTap {
    moduleId: string;
    portName: string;
    lane: number;
}

/** A track's send to one bus, as the mixer's send dial shows it. */
export interface VuSendDef {
    /** The bus's track name: the key of its meter. */
    bus: string;
    /**
     * Linear level (5 = full) when the dial controls it: a numeric level, a
     * bare bus name (5) or no send at all (0). Null when signal-driven.
     */
    level: number | null;
    /** `_smoothControl` driving the send's level; null when locked. */
    moduleId: string | null;
    /** True when the level is signal-driven, or the track sends to the bus
     *  more than once: the dial takes no input. */
    locked: boolean;
    /** Index of this send's live tap in the meter's `sendSources`. */
    sourceIndex?: number;
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
    /**
     * Pan position (-5 left … +5 right) when the panel's knob controls it —
     * stereo tracks whose `pan` option is a number or absent. Null when pan is
     * signal-driven or the track is mono (no knob).
     */
    pan: number | null;
    /** `_smoothControl` driving the stereo mixer's pan; null when `pan` is null. */
    panModuleId: string | null;
    /**
     * True when pan is signal-driven: the knob renders locked (externally
     * controlled) and tracks the live value from the meter frames.
     */
    panLocked: boolean;
    /**
     * Output gain in DSL units (audio taper; 5 = unity) when the panel's
     * fader controls it — tracks whose `gain` option is a number or absent.
     * Null when gain is signal-driven.
     */
    gain: number | null;
    /** `_smoothControl` driving the gain stage; null when `gain` is null. */
    gainModuleId: string | null;
    /** True when gain is signal-driven: the fader is locked and live. */
    gainLocked: boolean;
    /**
     * Live-display taps for locked controls (Rust samples them into the
     * meter frames). Absent when the control is editable or untappable.
     */
    panSource?: ControlTap;
    gainSource?: ControlTap;
    /**
     * One send per bus for a track or group, in no particular order; absent
     * on buses and the main strip. A track gets a send to every bus, at 0
     * when its source names none, so a dial is live from its first move.
     */
    sends?: VuSendDef[];
    /** Live taps of locked sends, sampled into the meter frames' `sends`. */
    sendSources?: ControlTap[];
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
    /** Send levels in the source by bus key, 0 where the source has none. */
    sends?: Record<string, number>;
}

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
