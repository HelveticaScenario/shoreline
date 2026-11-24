/** The knob's sweep: 135° either side of straight up. */
const SWEEP_DEGREES = 135;
/** The knob's centre and radius in its SVG's units. */
export const KNOB_CENTER = 11;
export const KNOB_RADIUS = 8;

/** The span a knob covers and the value its arc fills from. */
export interface KnobRange {
    min: number;
    max: number;
    /** Where the value arc starts: the centre detent of a pan knob, the
     *  bottom of a send dial. */
    origin: number;
}

/** A point on the knob's arc at `degrees` clockwise from straight up. */
function arcPoint(degrees: number): string {
    const rad = (degrees * Math.PI) / 180;
    return `${(KNOB_CENTER + KNOB_RADIUS * Math.sin(rad)).toFixed(2)} ${(KNOB_CENTER - KNOB_RADIUS * Math.cos(rad)).toFixed(2)}`;
}

/** The arc between two angles, drawn clockwise from the lesser. */
function arc(from: number, to: number): string {
    const [a, b] = from <= to ? [from, to] : [to, from];
    if (b - a < 0.5) {
        return '';
    }
    return `M ${arcPoint(a)} A ${KNOB_RADIUS} ${KNOB_RADIUS} 0 ${b - a > 180 ? 1 : 0} 1 ${arcPoint(b)}`;
}

/** The pointer's angle at `value`, clockwise from straight up. */
export function knobDegrees(value: number, { min, max }: KnobRange): number {
    const t = Math.min(1, Math.max(0, (value - min) / (max - min)));
    return -SWEEP_DEGREES + t * 2 * SWEEP_DEGREES;
}

/** The arc under the whole sweep. */
export const KNOB_TRACK_ARC = arc(-SWEEP_DEGREES, SWEEP_DEGREES);

/** The arc a knob fills from its range's origin to `value`. */
export function knobValueArc(value: number, range: KnobRange): string {
    return arc(knobDegrees(range.origin, range), knobDegrees(value, range));
}

/** The pointer's rotation at `value`, as an SVG transform. */
export function knobPointerTransform(value: number, range: KnobRange): string {
    return `rotate(${knobDegrees(value, range)} ${KNOB_CENTER} ${KNOB_CENTER})`;
}

/**
 * Point a knob's value arc and pointer at `value`. A locked knob is painted
 * through this from the meter loop, outside React.
 */
export function paintKnob(
    svg: SVGSVGElement,
    range: KnobRange,
    value: number,
): void {
    svg.querySelector('.vu-knob-value')?.setAttribute(
        'd',
        knobValueArc(value, range),
    );
    svg.querySelector('.vu-knob-pointer')?.setAttribute(
        'transform',
        knobPointerTransform(value, range),
    );
}
