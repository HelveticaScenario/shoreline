import React, { useRef } from 'react';
import type { SendGhostKey } from '../../shared/dsl/vuMeterTypes';

/** The knob's sweep: 135° either side of straight up. */
const SWEEP_DEGREES = 135;
const CENTER = 11;
const RADIUS = 8;
/** Vertical drag pixels across the knob's whole range. */
const DRAG_PIXELS = 100;

/** A track's knob: its pan, or its send to a bus. */
export type KnobId = 'pan' | SendGhostKey;

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
    return `${(CENTER + RADIUS * Math.sin(rad)).toFixed(2)} ${(CENTER - RADIUS * Math.cos(rad)).toFixed(2)}`;
}

/** The arc between two angles, drawn clockwise from the lesser. */
function arc(from: number, to: number): string {
    const [a, b] = from <= to ? [from, to] : [to, from];
    if (b - a < 0.5) {
        return '';
    }
    return `M ${arcPoint(a)} A ${RADIUS} ${RADIUS} 0 ${b - a > 180 ? 1 : 0} 1 ${arcPoint(b)}`;
}

function degreesOf(value: number, { min, max }: KnobRange): number {
    const t = Math.min(1, Math.max(0, (value - min) / (max - min)));
    return -SWEEP_DEGREES + t * 2 * SWEEP_DEGREES;
}

const TRACK_ARC = arc(-SWEEP_DEGREES, SWEEP_DEGREES);

/**
 * Point a knob's value arc and pointer at `value`. A locked knob is painted
 * through this from the meter loop, outside React; its range rides on the
 * SVG's data attributes.
 */
export function paintKnob(svg: SVGSVGElement, value: number): void {
    const range: KnobRange = {
        max: Number(svg.dataset.max),
        min: Number(svg.dataset.min),
        origin: Number(svg.dataset.origin),
    };
    const degrees = degreesOf(value, range);
    svg.querySelector('.vu-knob-value')?.setAttribute(
        'd',
        arc(degreesOf(range.origin, range), degrees),
    );
    svg.querySelector('.vu-knob-pointer')?.setAttribute(
        'transform',
        `rotate(${degrees} ${CENTER} ${CENTER})`,
    );
}

/**
 * A mixer knob. Vertical drag sets the value live and the release commits
 * it; right-click commits `resetValue`. A Ctrl/Cmd drag edits the source
 * only, showing a faded ghost pointer at the code-side value while it
 * diverges from the audio, and Ctrl/Cmd right-click reverts the source to
 * the audio's value. A locked knob (a signal-driven value) takes no input
 * and registers its SVG for the meter loop to paint.
 */
export function Knob({
    value,
    ghost,
    range,
    step,
    resetValue,
    locked,
    detent,
    className,
    title,
    onChange,
    onCommit,
    registerSvg,
}: {
    value: number;
    /** Code-side value while it diverges from the audio. */
    ghost: number | undefined;
    range: KnobRange;
    /** The value's rounding while dragging. */
    step: number;
    resetValue: number;
    locked: boolean;
    /** Marks the knob's centre with a triangle above it. */
    detent?: boolean;
    className: string;
    title: string;
    onChange: (value: number, codeOnly: boolean) => void;
    onCommit: (value: number, codeOnly: boolean) => void;
    registerSvg: (el: SVGSVGElement | null) => void;
}) {
    const dragRef = useRef<{
        startY: number;
        startValue: number;
        lastValue: number;
        codeOnly: boolean;
        moved: boolean;
    } | null>(null);

    const handlePointerDown = (e: React.PointerEvent<HTMLDivElement>) => {
        // Right presses belong to the contextmenu (reset/revert) gesture.
        if (e.button !== 0) {
            return;
        }
        e.preventDefault();
        e.currentTarget.setPointerCapture(e.pointerId);
        const codeOnly = e.ctrlKey || e.metaKey;
        // A code-only drag continues from the code-side value.
        const startValue = codeOnly ? (ghost ?? value) : value;
        dragRef.current = {
            codeOnly,
            lastValue: startValue,
            moved: false,
            startValue,
            startY: e.clientY,
        };
    };

    const handlePointerMove = (e: React.PointerEvent<HTMLDivElement>) => {
        const drag = dragRef.current;
        if (!drag) {
            return;
        }
        const raw =
            drag.startValue +
            ((drag.startY - e.clientY) * (range.max - range.min)) / DRAG_PIXELS;
        const clamped = Math.min(range.max, Math.max(range.min, raw));
        const next = Number((Math.round(clamped / step) * step).toFixed(6));
        if (next !== drag.lastValue) {
            drag.lastValue = next;
            drag.moved = true;
            onChange(next, drag.codeOnly);
        }
    };

    // Also bound to pointercancel/lostpointercapture, so a drag that loses
    // its capture still commits; after a normal pointerup it is a no-op. A
    // press that never moved the knob commits nothing.
    const handlePointerUp = () => {
        const drag = dragRef.current;
        dragRef.current = null;
        if (drag?.moved) {
            onCommit(drag.lastValue, drag.codeOnly);
        }
    };

    const degrees = degreesOf(value, range);
    const top = detent ? -5 : 0;
    return (
        <div
            className={`vu-knob ${className}${locked ? ' vu-knob--locked' : ''}`}
            title={title}
            onPointerDown={locked ? undefined : handlePointerDown}
            onPointerMove={locked ? undefined : handlePointerMove}
            onPointerUp={locked ? undefined : handlePointerUp}
            onPointerCancel={locked ? undefined : handlePointerUp}
            onLostPointerCapture={locked ? undefined : handlePointerUp}
            onContextMenu={
                locked
                    ? undefined
                    : (e) => {
                          e.preventDefault();
                          if (e.ctrlKey || e.metaKey) {
                              // Revert the source to the audio's value; no-op
                              // when they already agree.
                              if (ghost !== undefined) {
                                  onCommit(value, true);
                              }
                              return;
                          }
                          onCommit(resetValue, false);
                      }
            }
        >
            <svg
                ref={locked ? registerSvg : undefined}
                viewBox={`0 ${top} 22 ${22 - top}`}
                data-min={range.min}
                data-max={range.max}
                data-origin={range.origin}
            >
                {detent && (
                    <path
                        d="M8 -5 L14 -5 L11 -1.5 Z"
                        className="vu-knob-detent"
                    />
                )}
                <path className="vu-knob-track" d={TRACK_ARC} />
                {ghost !== undefined && (
                    <line
                        className="vu-knob-ghost"
                        x1={CENTER}
                        y1={CENTER}
                        x2={CENTER}
                        y2={CENTER - RADIUS + 1}
                        transform={`rotate(${degreesOf(ghost, range)} ${CENTER} ${CENTER})`}
                    />
                )}
                <path
                    className="vu-knob-value"
                    d={arc(degreesOf(range.origin, range), degrees)}
                />
                <line
                    className="vu-knob-pointer"
                    x1={CENTER}
                    y1={CENTER}
                    x2={CENTER}
                    y2={CENTER - RADIUS + 1}
                    transform={`rotate(${degrees} ${CENTER} ${CENTER})`}
                />
            </svg>
        </div>
    );
}
