import { GATE_HIGH_VOLTAGE } from './buttonTypes';

/**
 * The ranges the editor's control modules declare through their `min` and
 * `max` params, which `.range()` maps from. A live edit replaces a module's
 * params whole, so it sends the range again beside the value.
 */
export type ControlRange = { readonly max: number; readonly min: number };

/** Buttons and an out's mute/solo gate: 0 V or 5 V. */
export const GATE_RANGE: ControlRange = { max: GATE_HIGH_VOLTAGE, min: 0 };

/** An out's gain fader, in DSL units (5 = unity). */
export const OUT_GAIN_RANGE: ControlRange = { max: 10, min: 0 };

/** An out's pan knob: -5 left … +5 right. */
export const OUT_PAN_RANGE: ControlRange = { max: 5, min: -5 };

/** A video region's average: 0 dark … 1 bright. */
export const VIDEO_CV_RANGE: ControlRange = { max: 1, min: 0 };

/** A control module's whole params: its value and the range it declares. */
export type ControlParams = ControlRange & { readonly value: number };

/** The params a live edit sends a control declaring `range`. */
export function controlParams(
    range: ControlRange,
    value: number,
): ControlParams {
    return { max: range.max, min: range.min, value };
}

/** A button's or mute/solo gate's live params. */
export const gateParams = (value: number) => controlParams(GATE_RANGE, value);

/** An out gain fader's live params. */
export const outGainParams = (value: number) =>
    controlParams(OUT_GAIN_RANGE, value);

/** An out pan knob's live params. */
export const outPanParams = (value: number) =>
    controlParams(OUT_PAN_RANGE, value);

/** A video region's live params. */
export const videoCvParams = (value: number) =>
    controlParams(VIDEO_CV_RANGE, value);
