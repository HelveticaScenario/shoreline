import { GATE_HIGH_VOLTAGE } from './buttonTypes';

/**
 * The ranges the editor's control modules declare through their `min` and
 * `max` params, which `.range()` maps from. A live edit replaces a module's
 * params whole, so it sends the range again beside the value.
 */
export interface ControlRange {
    readonly max: number;
    readonly min: number;
}

/** Buttons and an out's mute/solo gate: 0 V or 5 V. */
export const GATE_RANGE: ControlRange = { max: GATE_HIGH_VOLTAGE, min: 0 };

/** An out's gain fader, in DSL units (5 = unity). */
export const OUT_GAIN_RANGE: ControlRange = { max: 10, min: 0 };

/** An out's pan knob: -5 left … +5 right. */
export const OUT_PAN_RANGE: ControlRange = { max: 5, min: -5 };

/** A video region's average: 0 dark … 1 bright. */
export const VIDEO_CV_RANGE: ControlRange = { max: 1, min: 0 };
