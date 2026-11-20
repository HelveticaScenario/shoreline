import React, { useCallback, useRef } from 'react';
import type { VuSendDef } from '../../shared/dsl/vuMeterTypes';

/** Height of one row of send dials, letter included. */
export const SEND_ROW_HEIGHT = 32;
/** Send dials per row of a strip. */
export const SENDS_PER_ROW = 3;
/** Send level change per vertical drag pixel (the whole 0–5 over 100px). */
const LEVEL_PER_PIXEL = 0.05;
const MAX_LEVEL = 5;

/** The dial's sweep: 135° either side of straight up. */
const SWEEP_DEGREES = 135;
const CENTER = 11;
const RADIUS = 8;

/** A point on the dial's arc at `degrees` clockwise from straight up. */
function arcPoint(degrees: number): string {
    const rad = (degrees * Math.PI) / 180;
    return `${(CENTER + RADIUS * Math.sin(rad)).toFixed(2)} ${(CENTER - RADIUS * Math.cos(rad)).toFixed(2)}`;
}

/** The arc from the dial's bottom-left stop to `degrees`. */
function arcTo(degrees: number): string {
    const large = degrees + SWEEP_DEGREES > 180 ? 1 : 0;
    return `M ${arcPoint(-SWEEP_DEGREES)} A ${RADIUS} ${RADIUS} 0 ${large} 1 ${arcPoint(degrees)}`;
}

function levelDegrees(level: number): number {
    const t = Math.min(1, Math.max(0, level / MAX_LEVEL));
    return -SWEEP_DEGREES + t * 2 * SWEEP_DEGREES;
}

const TRACK_ARC = arcTo(SWEEP_DEGREES);

/**
 * Point a dial's value arc and pointer at `level`. Locked dials are painted
 * through this from the meter loop, outside React.
 */
export function paintSendDial(svg: SVGSVGElement, level: number): void {
    const degrees = levelDegrees(level);
    svg.querySelector('.vu-send-value')?.setAttribute(
        'd',
        level > 0 ? arcTo(degrees) : '',
    );
    svg.querySelector('.vu-send-pointer')?.setAttribute(
        'transform',
        `rotate(${degrees} ${CENTER} ${CENTER})`,
    );
}

/**
 * One send dial: vertical drag sets the level live, and the release commits
 * it; right-click commits 0, removing the send. A Ctrl/Cmd drag edits the
 * source only, showing a faded ghost at the code-side level while it
 * diverges from the audio; Ctrl/Cmd right-click reverts the source to the
 * audio's level. A locked dial (a signal-driven level) takes no input and
 * registers its SVG for the meter loop to paint.
 */
function SendDial({
    letter,
    busLabel,
    send,
    ghostLevel,
    onChange,
    onCommit,
    registerDial,
}: {
    letter: string;
    busLabel: string;
    send: VuSendDef;
    /** Code-side level while it diverges from the audio. */
    ghostLevel: number | undefined;
    onChange: (level: number, codeOnly: boolean) => void;
    onCommit: (level: number, codeOnly: boolean) => void;
    registerDial: (el: SVGSVGElement | null) => void;
}) {
    const dragRef = useRef<{
        startY: number;
        startLevel: number;
        lastLevel: number;
        codeOnly: boolean;
        moved: boolean;
    } | null>(null);
    const level = send.level ?? 0;
    const editable = !send.locked && send.moduleId !== null;

    const handlePointerDown = useCallback(
        (e: React.PointerEvent<HTMLDivElement>) => {
            if (e.button !== 0) {
                return;
            }
            e.preventDefault();
            e.currentTarget.setPointerCapture(e.pointerId);
            const codeOnly = e.ctrlKey || e.metaKey;
            // A code-only drag continues from the code-side level.
            const startLevel = codeOnly ? (ghostLevel ?? level) : level;
            dragRef.current = {
                codeOnly,
                lastLevel: startLevel,
                moved: false,
                startLevel,
                startY: e.clientY,
            };
        },
        [level, ghostLevel],
    );

    const handlePointerMove = useCallback(
        (e: React.PointerEvent<HTMLDivElement>) => {
            const drag = dragRef.current;
            if (!drag) {
                return;
            }
            const raw =
                drag.startLevel + (drag.startY - e.clientY) * LEVEL_PER_PIXEL;
            const next =
                Math.round(Math.min(MAX_LEVEL, Math.max(0, raw)) * 100) / 100;
            if (next !== drag.lastLevel) {
                drag.lastLevel = next;
                drag.moved = true;
                onChange(next, drag.codeOnly);
            }
        },
        [onChange],
    );

    // Also bound to pointercancel/lostpointercapture, so a drag that loses
    // its capture still commits; after a normal pointerup it is a no-op. A
    // press that never moved the dial commits nothing.
    const handlePointerUp = useCallback(() => {
        const drag = dragRef.current;
        dragRef.current = null;
        if (drag?.moved) {
            onCommit(drag.lastLevel, drag.codeOnly);
        }
    }, [onCommit]);

    const degrees = levelDegrees(level);
    return (
        <div
            className={`vu-send${send.locked ? ' vu-send--locked' : ''}`}
            title={
                send.locked
                    ? `Send to ${busLabel} (${send.level === null ? 'signal-controlled' : `${send.level}, set in code`})`
                    : `Send to ${busLabel}: ${level} (drag; ⌃/⌘ drag: edit code only; right-click: off; ⌃/⌘ right-click: revert)`
            }
            onPointerDown={editable ? handlePointerDown : undefined}
            onPointerMove={editable ? handlePointerMove : undefined}
            onPointerUp={editable ? handlePointerUp : undefined}
            onPointerCancel={editable ? handlePointerUp : undefined}
            onLostPointerCapture={editable ? handlePointerUp : undefined}
            onContextMenu={
                editable
                    ? (e) => {
                          e.preventDefault();
                          if (e.ctrlKey || e.metaKey) {
                              // Revert the source to the audio's level; no-op
                              // when they already agree.
                              if (ghostLevel !== undefined) {
                                  onCommit(level, true);
                              }
                              return;
                          }
                          onCommit(0, false);
                      }
                    : undefined
            }
        >
            <span className="vu-send-letter">{letter}</span>
            <svg
                ref={send.locked ? registerDial : undefined}
                viewBox="0 0 22 22"
            >
                <path className="vu-send-track" d={TRACK_ARC} />
                {ghostLevel !== undefined && (
                    <line
                        className="vu-send-ghost"
                        x1={CENTER}
                        y1={CENTER}
                        x2={CENTER}
                        y2={CENTER - RADIUS + 1}
                        transform={`rotate(${levelDegrees(ghostLevel)} ${CENTER} ${CENTER})`}
                    />
                )}
                <path
                    className="vu-send-value"
                    d={level > 0 ? arcTo(degrees) : ''}
                />
                <line
                    className="vu-send-pointer"
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

/** A bus a strip can send to: its key, the letter it shows, its name. */
export interface SendBus {
    key: string;
    letter: string;
    label: string;
}

/**
 * A strip's send dials, one per bus in the order the buses are shown. A
 * strip with no sends (a bus, the main) keeps the section's space empty so
 * every strip's meter lines up.
 */
export function VuSendDials({
    trackKey,
    buses,
    sends,
    ghosts,
    onSendChange,
    onSendCommit,
    registerSendDial,
    unregisterSendDial,
}: {
    trackKey: string;
    buses: SendBus[];
    sends: VuSendDef[] | undefined;
    /** Code-side levels diverging from the audio, by bus key. */
    ghosts: Record<string, number> | undefined;
    onSendChange: (
        key: string,
        bus: string,
        level: number,
        codeOnly: boolean,
    ) => void;
    onSendCommit: (
        key: string,
        bus: string,
        level: number,
        codeOnly: boolean,
    ) => void;
    registerSendDial: (key: string, bus: string, el: SVGSVGElement) => void;
    unregisterSendDial: (key: string, bus: string) => void;
}) {
    const rows = Math.ceil(buses.length / SENDS_PER_ROW);
    return (
        <div
            className="vu-sends"
            style={{ height: rows * SEND_ROW_HEIGHT }}
            aria-hidden={sends === undefined}
        >
            {sends !== undefined &&
                buses.map((bus) => {
                    const send = sends.find((s) => s.bus === bus.key);
                    if (!send) {
                        return <div key={bus.key} />;
                    }
                    return (
                        <SendDial
                            key={`${bus.key}:${send.locked}`}
                            letter={bus.letter}
                            busLabel={bus.label}
                            send={send}
                            ghostLevel={ghosts?.[bus.key]}
                            onChange={(level, codeOnly) =>
                                onSendChange(trackKey, bus.key, level, codeOnly)
                            }
                            onCommit={(level, codeOnly) =>
                                onSendCommit(trackKey, bus.key, level, codeOnly)
                            }
                            registerDial={(el) =>
                                el
                                    ? registerSendDial(trackKey, bus.key, el)
                                    : unregisterSendDial(trackKey, bus.key)
                            }
                        />
                    );
                })}
        </div>
    );
}
