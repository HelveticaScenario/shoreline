import React, { useEffect, useRef } from 'react';
import { OUT_PAN_RANGE } from '../../shared/dsl/controlRanges';
import type { VuMeterDef, VuMeterGhost } from '../../shared/dsl/vuMeterTypes';
import { METER_PAD_CSS, meterNormToDb } from '../app/vuMeter';
import type { MixerController } from '../mixer/MixerController';
import {
    controlTooltip,
    isJumpGesture,
    jumpHandlers,
} from './controlPanel/controlChrome';
import { Knob, type KnobRange } from './VuKnob';
import { type SendBus, VuSendDials } from './VuSendDials';

/**
 * A Ctrl/Cmd gesture on a control edits the DSL source only, leaving the
 * running audio untouched until the next patch update; the control then
 * shows a ghost of the code-side value. On macOS Ctrl+click arrives as a
 * contextmenu event, so the buttons detect the code-only gesture on that path
 * too. On a strip's name the same gesture jumps to its label.
 */
const isCodeOnly = isJumpGesture;

const MS_BUTTON_HINT = '⌃/⌘-click: edit code only';

const PAN_RANGE: KnobRange = { ...OUT_PAN_RANGE, origin: 0 };

/** Map a pointer event on a meter's canvas to a dB position on its scale. */
function eventToDb(e: React.PointerEvent<HTMLCanvasElement>): number {
    const rect = e.currentTarget.getBoundingClientRect();
    const norm =
        (e.clientY - rect.top - METER_PAD_CSS) /
        Math.max(1, rect.height - METER_PAD_CSS * 2);
    return meterNormToDb(norm);
}

interface VuMeterProps {
    mixer: MixerController;
    output: VuMeterDef;
    /** Code-side values diverging from the audio for this meter. */
    ghost: VuMeterGhost | undefined;
    /** Shown on the activator button: a track's number, a bus's letter. */
    index: number | string;
    /** True when mute/solo state leaves this output inaudible. */
    suppressed: boolean;
    /** For a group's strip: whether its members are hidden, and the toggle. */
    fold?: { folded: boolean; onToggle: () => void };
    showReadout: boolean;
    showPan: boolean;
    /** The buses every track's send dials follow, or null to show none. */
    sendBuses: SendBus[] | null;
    /** Ctrl/Cmd-click on the strip's name: move the cursor to its label. */
    onJump: (key: string) => void;
}

export function VuMeter({
    mixer,
    output,
    ghost,
    index,
    suppressed,
    fold,
    showReadout,
    showPan,
    sendBuses,
    onJump,
}: VuMeterProps) {
    const canvasRef = useRef<HTMLCanvasElement | null>(null);
    /** Non-null while a fader drag is live; the modifier state is sampled
     *  once at pointerdown and holds for the whole drag. */
    const faderDragRef = useRef<{ codeOnly: boolean } | null>(null);

    // Size the backing store to device pixels and keep it matched while the
    // panel is resized. Reassigning canvas.width clears the canvas, so only
    // touch it on a real change, and ask for an immediate redraw after —
    // the RAF loop only repaints while the clock runs. Registration comes
    // first so the mount-time redraw finds the canvas and paints the panel
    // as soon as it opens.
    useEffect(() => {
        const canvas = canvasRef.current;
        if (!canvas) {
            return;
        }
        canvas.dataset.tapModuleId = output.moduleId;
        canvas.dataset.channels = String(output.channels);
        mixer.meters.registerCanvas(output.key, canvas);
        const resize = () => {
            const dpr = window.devicePixelRatio || 1;
            const rect = canvas.getBoundingClientRect();
            const width = Math.max(1, Math.round(rect.width * dpr));
            const height = Math.max(1, Math.round(rect.height * dpr));
            if (canvas.width === width && canvas.height === height) {
                return;
            }
            canvas.width = width;
            canvas.height = height;
            mixer.meters.redraw(output.key);
        };
        resize();
        // First paint regardless of whether the default backing-store size
        // happened to match.
        mixer.meters.redraw(output.key);
        const observer = new ResizeObserver(resize);
        observer.observe(canvas);
        return () => {
            observer.disconnect();
            mixer.meters.unregisterCanvas(output.key);
        };
    }, [mixer, output.key, output.moduleId, output.channels]);

    const faderEnabled = output.gain.moduleId !== null;

    const handleFaderDown = (e: React.PointerEvent<HTMLCanvasElement>) => {
        // Right presses belong to the contextmenu (reset/revert) gesture.
        if (!faderEnabled || e.button !== 0) {
            return;
        }
        e.preventDefault();
        e.currentTarget.setPointerCapture(e.pointerId);
        const codeOnly = isCodeOnly(e);
        faderDragRef.current = { codeOnly };
        mixer.changeGain(output.key, eventToDb(e), codeOnly);
    };

    const handleFaderMove = (e: React.PointerEvent<HTMLCanvasElement>) => {
        const drag = faderDragRef.current;
        if (drag) {
            mixer.changeGain(output.key, eventToDb(e), drag.codeOnly);
        }
    };

    // Also bound to pointercancel/lostpointercapture: a drag that loses its
    // capture must end here, or button-less hover moves keep rewriting the
    // gain and the final value never flushes to the source. Re-entry after
    // a normal pointerup is a no-op (the drag ref is already null).
    const handleFaderUp = () => {
        const drag = faderDragRef.current;
        if (drag) {
            faderDragRef.current = null;
            mixer.commitGain(output.key, drag.codeOnly);
        }
    };

    const readoutRef = (el: HTMLButtonElement | null) => {
        if (el) {
            mixer.meters.registerReadout(output.key, el);
        } else {
            mixer.meters.unregisterReadout(output.key);
        }
    };

    const channelBadge =
        output.channels === 2
            ? `ch ${output.baseChannel}–${output.baseChannel + 1}`
            : `ch ${output.baseChannel}`;

    const title = output.main
        ? 'End of chain'
        : `${output.key} (${channelBadge})`;

    return (
        <div
            className={`vu-meter${suppressed ? ' vu-meter--suppressed' : ''}${
                output.main ? ' vu-meter--main' : ''
            }`}
            data-vu-key={output.key}
            title={title}
        >
            <div
                className="vu-meter-header"
                {...(!output.main && {
                    ...jumpHandlers(output.key, onJump),
                    title: controlTooltip(title),
                })}
            >
                {fold && (
                    <button
                        className="vu-group-fold"
                        aria-expanded={!fold.folded}
                        title={fold.folded ? 'Show tracks' : 'Hide tracks'}
                        onClick={fold.onToggle}
                    >
                        {fold.folded ? '▸' : '▾'}
                    </button>
                )}
                <span className="vu-meter-label">
                    {output.label ?? output.key}
                </span>
            </div>
            {sendBuses && (
                <VuSendDials
                    trackKey={output.key}
                    buses={sendBuses}
                    sends={output.sends}
                    ghost={ghost}
                    mixer={mixer}
                />
            )}
            <div className="vu-meter-body">
                <div className="vu-meter-controls">
                    {showReadout && (
                        <button
                            ref={readoutRef}
                            className="vu-peak-readout"
                            title="Highest peak since reset, red above 0 dB (click to reset)"
                            onClick={() => mixer.meters.resetPeak(output.key)}
                        >
                            -∞
                        </button>
                    )}
                    <div className="vu-meter-controls-spacer" />
                    {showPan && output.pan && (
                        // Remounted when its lock changes, which drops what
                        // the meter loop painted onto it.
                        <Knob
                            key={`pan:${output.pan.locked}`}
                            className="vu-pan-knob"
                            value={output.pan.value ?? 0}
                            ghost={ghost?.pan}
                            range={PAN_RANGE}
                            step={0.1}
                            resetValue={0}
                            locked={output.pan.locked}
                            detent
                            title={
                                output.pan.locked
                                    ? 'Pan (signal-controlled)'
                                    : `Pan ${output.pan.value} (drag; ⌃/⌘ drag: edit code only; right-click: center; ⌃/⌘ right-click: revert)`
                            }
                            onChange={(value, codeOnly) =>
                                mixer.changeKnob(
                                    output.key,
                                    'pan',
                                    value,
                                    codeOnly,
                                )
                            }
                            onCommit={(value, codeOnly) =>
                                mixer.commitKnob(
                                    output.key,
                                    'pan',
                                    value,
                                    codeOnly,
                                )
                            }
                            registerSvg={(el) =>
                                el
                                    ? mixer.meters.registerKnob(
                                          output.key,
                                          'pan',
                                          el,
                                      )
                                    : mixer.meters.unregisterKnob(
                                          output.key,
                                          'pan',
                                      )
                            }
                        />
                    )}
                    {output.muteModuleId != null && (
                        <>
                            {/* Outer ring = code state, inner core = audio
                                state; they only differ while a code-only
                                edit awaits a patch update. */}
                            <button
                                className="vu-btn vu-btn-mute"
                                aria-pressed={output.mute}
                                data-code-pressed={ghost?.mute ?? output.mute}
                                title={`${output.mute ? 'Unmute' : 'Mute'} (${MS_BUTTON_HINT})`}
                                onClick={(e) =>
                                    mixer.toggle(
                                        output.key,
                                        'mute',
                                        isCodeOnly(e),
                                    )
                                }
                                onContextMenu={(e) => {
                                    if (e.ctrlKey) {
                                        e.preventDefault();
                                        e.stopPropagation();
                                        mixer.toggle(output.key, 'mute', true);
                                    }
                                }}
                            >
                                <span className="vu-btn-inner">{index}</span>
                            </button>
                            {output.isBus ? (
                                // A bus has no solo; the gap keeps its
                                // activator level with the tracks'.
                                <div
                                    className="vu-btn vu-btn-gap"
                                    aria-hidden="true"
                                >
                                    <span className="vu-btn-inner">S</span>
                                </div>
                            ) : (
                                <button
                                    className="vu-btn vu-btn-solo"
                                    aria-pressed={output.solo}
                                    data-code-pressed={
                                        ghost?.solo ?? output.solo
                                    }
                                    title={`Solo (${MS_BUTTON_HINT})`}
                                    onClick={(e) =>
                                        mixer.toggle(
                                            output.key,
                                            'solo',
                                            isCodeOnly(e),
                                        )
                                    }
                                    onContextMenu={(e) => {
                                        if (e.ctrlKey) {
                                            e.preventDefault();
                                            e.stopPropagation();
                                            mixer.toggle(
                                                output.key,
                                                'solo',
                                                true,
                                            );
                                        }
                                    }}
                                >
                                    <span className="vu-btn-inner">S</span>
                                </button>
                            )}
                        </>
                    )}
                </div>
                <div className="vu-meter-canvas-wrap">
                    <canvas
                        ref={canvasRef}
                        className={`vu-meter-canvas${
                            faderEnabled ? ' vu-meter-canvas--fader' : ''
                        }${output.gain.locked ? ' vu-meter-canvas--locked' : ''}`}
                        title={
                            output.gain.locked
                                ? 'Gain (signal-controlled)'
                                : faderEnabled
                                  ? 'Gain (drag; ⌃/⌘ drag: edit code only; right-click: reset; ⌃/⌘ right-click: revert)'
                                  : undefined
                        }
                        onPointerDown={handleFaderDown}
                        onPointerMove={handleFaderMove}
                        onPointerUp={handleFaderUp}
                        onPointerCancel={handleFaderUp}
                        onLostPointerCapture={handleFaderUp}
                        onContextMenu={
                            faderEnabled
                                ? (e) => {
                                      e.preventDefault();
                                      if (isCodeOnly(e)) {
                                          mixer.revertGain(output.key);
                                      } else {
                                          mixer.resetGain(output.key);
                                      }
                                  }
                                : undefined
                        }
                    />
                </div>
            </div>
        </div>
    );
}
