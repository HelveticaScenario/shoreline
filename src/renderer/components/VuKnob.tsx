import React, { useRef } from 'react';
import {
    KNOB_CENTER,
    KNOB_RADIUS,
    KNOB_TRACK_ARC,
    type KnobRange,
    knobPointerTransform,
    knobValueArc,
} from '../mixer/knobPaint';
import type { MixerController } from '../mixer/MixerController';
import type { KnobId } from '../mixer/mixerKnobId';
import { isJumpGesture } from './controlPanel/controlChrome';

/** Vertical drag pixels across the knob's whole range. */
const DRAG_PIXELS = 100;

/**
 * Knob `id` of the track named `trackKey`. Vertical drag sets the value live
 * and the release commits it; right-click commits `resetValue`. A Ctrl/Cmd
 * drag edits the source only, showing a faded ghost pointer at the code-side
 * value while it diverges from the audio, and Ctrl/Cmd right-click reverts
 * the source to the audio's value. A locked knob (a signal-driven value)
 * takes no input and registers its SVG for the meter loop to paint.
 */
export function Knob({
    mixer,
    trackKey,
    id,
    value,
    ghost,
    range,
    step,
    resetValue,
    locked,
    detent,
    className,
    title,
}: {
    mixer: MixerController;
    trackKey: string;
    id: KnobId;
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
}) {
    const dragRef = useRef<{
        startY: number;
        startValue: number;
        lastValue: number;
        codeOnly: boolean;
        moved: boolean;
    } | null>(null);

    const commit = (next: number, codeOnly: boolean) =>
        mixer.knobs.commit(trackKey, id, next, codeOnly);

    const registerSvg = (el: SVGSVGElement | null) => {
        if (el) {
            mixer.meters.registerKnob(trackKey, id, el, range);
        } else {
            mixer.meters.unregisterKnob(trackKey, id);
        }
    };

    const handlePointerDown = (e: React.PointerEvent<HTMLDivElement>) => {
        // Right presses belong to the contextmenu (reset/revert) gesture.
        if (e.button !== 0) {
            return;
        }
        e.preventDefault();
        e.currentTarget.setPointerCapture(e.pointerId);
        const codeOnly = isJumpGesture(e);
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
            mixer.knobs.change(trackKey, id, next, drag.codeOnly);
        }
    };

    // Also bound to pointercancel/lostpointercapture, so a drag that loses
    // its capture still commits; after a normal pointerup it is a no-op. A
    // press that never moved the knob commits nothing.
    const handlePointerUp = () => {
        const drag = dragRef.current;
        dragRef.current = null;
        if (drag?.moved) {
            commit(drag.lastValue, drag.codeOnly);
        }
    };

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
                          if (isJumpGesture(e)) {
                              // Revert the source to the audio's value; no-op
                              // when they already agree.
                              if (ghost !== undefined) {
                                  commit(value, true);
                              }
                              return;
                          }
                          commit(resetValue, false);
                      }
            }
        >
            <svg
                ref={locked ? registerSvg : undefined}
                viewBox={`0 ${top} 22 ${22 - top}`}
            >
                {detent && (
                    <path
                        d="M8 -5 L14 -5 L11 -1.5 Z"
                        className="vu-knob-detent"
                    />
                )}
                <path className="vu-knob-track" d={KNOB_TRACK_ARC} />
                {ghost !== undefined && (
                    <line
                        className="vu-knob-ghost"
                        x1={KNOB_CENTER}
                        y1={KNOB_CENTER}
                        x2={KNOB_CENTER}
                        y2={KNOB_CENTER - KNOB_RADIUS + 1}
                        transform={knobPointerTransform(ghost, range)}
                    />
                )}
                <path
                    className="vu-knob-value"
                    d={knobValueArc(value, range)}
                />
                <line
                    className="vu-knob-pointer"
                    x1={KNOB_CENTER}
                    y1={KNOB_CENTER}
                    x2={KNOB_CENTER}
                    y2={KNOB_CENTER - KNOB_RADIUS + 1}
                    transform={knobPointerTransform(value, range)}
                />
            </svg>
        </div>
    );
}
