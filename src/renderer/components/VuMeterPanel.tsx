import React, {
    useCallback,
    useEffect,
    useLayoutEffect,
    useRef,
    useState,
    useSyncExternalStore,
} from 'react';
import { OUT_PAN_RANGE } from '../../shared/dsl/controlRanges';
import type { VuMeterDef, VuMeterGhost } from '../../shared/dsl/vuMeterTypes';
import { trackGates } from '../../shared/dsl/trackGates';
import { METER_PAD_CSS, meterNormToDb } from '../app/vuMeter';
import {
    controlTooltip,
    jumpHandlers,
    useJumpModifier,
} from './controlPanel/controlChrome';
import { Knob, type KnobHandlers, type KnobRange } from './VuKnob';
import {
    type SendBus,
    SEND_ROW_HEIGHT,
    SENDS_PER_ROW,
    VuSendDials,
} from './VuSendDials';
import './VuMeterPanel.css';

/**
 * A Ctrl/Cmd gesture edits the DSL source only, leaving the running audio
 * untouched until the next patch update; the control then shows a ghost of
 * the code-side value. On macOS Ctrl+click arrives as a contextmenu event,
 * so the buttons detect the code-only gesture on that path too.
 */
function isCodeOnly(e: React.MouseEvent | React.PointerEvent): boolean {
    return e.ctrlKey || e.metaKey;
}

const MS_BUTTON_HINT = '⌃/⌘-click: edit code only';

/** Strip height at which the pan knob appears. */
const PAN_MIN_HEIGHT = 150;
/** Strip height at which the peak-readout pill appears. */
const READOUT_MIN_HEIGHT = 180;
/** Panel height left for a strip's pan, mute and solo once its send dials
 *  are shown; each row of dials needs this plus a row's height, so the
 *  sends give way before the pan knob does. */
const SENDS_MIN_HEIGHT = 140;
const PAN_RANGE: KnobRange = { ...OUT_PAN_RANGE, origin: 0 };

interface VuMeterPanelProps extends KnobHandlers {
    outputs: VuMeterDef[];
    onToggleMute: (key: string, codeOnly: boolean) => void;
    onToggleSolo: (key: string, codeOnly: boolean) => void;
    /** Code-side values diverging from the audio, keyed by meter key. */
    ghosts: Map<string, VuMeterGhost>;
    /** Live fader drag on the meter: absolute position mapped to dB. */
    onGainChange: (key: string, db: number, codeOnly: boolean) => void;
    /** Drag released — flush the final gain into the source once. */
    onGainCommit: (key: string, codeOnly: boolean) => void;
    /** Right-click on the meter: reset gain to the default. */
    onGainReset: (key: string) => void;
    /** Ctrl/Cmd right-click: revert the source gain to the audio's. */
    onGainRevert: (key: string) => void;
    onPeakReset: (key: string) => void;
    /** The canvas backing store changed size and needs an immediate redraw. */
    onCanvasResized: (key: string, canvas: HTMLCanvasElement) => void;
    registerCanvas: (key: string, canvas: HTMLCanvasElement) => void;
    unregisterCanvas: (key: string) => void;
    registerReadout: (key: string, el: HTMLElement) => void;
    unregisterReadout: (key: string) => void;
    /** Ctrl/Cmd-click on a strip's name: move the cursor to its label. */
    onJump: (key: string) => void;
}

/**
 * The groups whose members are hidden, by key. Kept outside the panel, which
 * unmounts with its tab, and for as long as the app runs.
 */
const foldStore = {
    folded: new Set<string>() as ReadonlySet<string>,
    listeners: new Set<() => void>(),
    subscribe: (listener: () => void): (() => void) => {
        foldStore.listeners.add(listener);
        return () => foldStore.listeners.delete(listener);
    },
    toggle(key: string): void {
        const next = new Set(foldStore.folded);
        if (!next.delete(key)) {
            next.add(key);
        }
        foldStore.folded = next;
        foldStore.listeners.forEach((listener) => listener());
    },
};

/** The letter a bus shows on its activator: A, B, … Z, AA, AB, … */
function busLetter(index: number): string {
    const letter = String.fromCharCode(65 + (index % 26));
    return index < 26 ? letter : busLetter(Math.floor(index / 26) - 1) + letter;
}

interface VuMeterProps extends KnobHandlers {
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
    onToggleMute: (key: string, codeOnly: boolean) => void;
    onToggleSolo: (key: string, codeOnly: boolean) => void;
    onGainChange: (key: string, db: number, codeOnly: boolean) => void;
    onGainCommit: (key: string, codeOnly: boolean) => void;
    onGainReset: (key: string) => void;
    onGainRevert: (key: string) => void;
    onPeakReset: (key: string) => void;
    onCanvasResized: (key: string, canvas: HTMLCanvasElement) => void;
    registerCanvas: (key: string, canvas: HTMLCanvasElement) => void;
    unregisterCanvas: (key: string) => void;
    registerReadout: (key: string, el: HTMLElement) => void;
    unregisterReadout: (key: string) => void;
    onJump: (key: string) => void;
}

function VuMeter({
    output,
    ghost,
    index,
    suppressed,
    fold,
    showReadout,
    showPan,
    sendBuses,
    onToggleMute,
    onToggleSolo,
    onGainChange,
    onGainCommit,
    onGainReset,
    onGainRevert,
    onPeakReset,
    onCanvasResized,
    registerCanvas,
    unregisterCanvas,
    registerReadout,
    unregisterReadout,
    onJump,
    ...knobs
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
        registerCanvas(output.key, canvas);
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
            onCanvasResized(output.key, canvas);
        };
        resize();
        // First paint regardless of whether the default backing-store size
        // happened to match.
        onCanvasResized(output.key, canvas);
        const observer = new ResizeObserver(resize);
        observer.observe(canvas);
        return () => {
            observer.disconnect();
            unregisterCanvas(output.key);
        };
    }, [
        output.key,
        output.moduleId,
        output.channels,
        onCanvasResized,
        registerCanvas,
        unregisterCanvas,
    ]);

    /** Map a pointer event on the canvas to a dB position on the scale. */
    const eventToDb = useCallback(
        (e: React.PointerEvent<HTMLCanvasElement>) => {
            const rect = e.currentTarget.getBoundingClientRect();
            const norm =
                (e.clientY - rect.top - METER_PAD_CSS) /
                Math.max(1, rect.height - METER_PAD_CSS * 2);
            return meterNormToDb(norm);
        },
        [],
    );

    const faderEnabled = output.gain.moduleId !== null;

    const handleFaderDown = useCallback(
        (e: React.PointerEvent<HTMLCanvasElement>) => {
            // Right presses belong to the contextmenu (reset/revert) gesture.
            if (!faderEnabled || e.button !== 0) {
                return;
            }
            e.preventDefault();
            e.currentTarget.setPointerCapture(e.pointerId);
            const codeOnly = isCodeOnly(e);
            faderDragRef.current = { codeOnly };
            onGainChange(output.key, eventToDb(e), codeOnly);
        },
        [faderEnabled, output.key, onGainChange, eventToDb],
    );

    const handleFaderMove = useCallback(
        (e: React.PointerEvent<HTMLCanvasElement>) => {
            const drag = faderDragRef.current;
            if (drag) {
                onGainChange(output.key, eventToDb(e), drag.codeOnly);
            }
        },
        [output.key, onGainChange, eventToDb],
    );

    // Also bound to pointercancel/lostpointercapture: a drag that loses its
    // capture must end here, or button-less hover moves keep rewriting the
    // gain and the final value never flushes to the source. Re-entry after
    // a normal pointerup is a no-op (the drag ref is already null).
    const handleFaderUp = useCallback(() => {
        const drag = faderDragRef.current;
        if (drag) {
            faderDragRef.current = null;
            onGainCommit(output.key, drag.codeOnly);
        }
    }, [output.key, onGainCommit]);

    const readoutRef = useCallback(
        (el: HTMLButtonElement | null) => {
            if (el) {
                registerReadout(output.key, el);
            } else {
                unregisterReadout(output.key);
            }
        },
        [output.key, registerReadout, unregisterReadout],
    );

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
                    {...knobs}
                />
            )}
            <div className="vu-meter-body">
                <div className="vu-meter-controls">
                    {showReadout && (
                        <button
                            ref={readoutRef}
                            className="vu-peak-readout"
                            title="Highest peak since reset, red above 0 dB (click to reset)"
                            onClick={() => onPeakReset(output.key)}
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
                                knobs.onKnobChange(
                                    output.key,
                                    'pan',
                                    value,
                                    codeOnly,
                                )
                            }
                            onCommit={(value, codeOnly) =>
                                knobs.onKnobCommit(
                                    output.key,
                                    'pan',
                                    value,
                                    codeOnly,
                                )
                            }
                            registerSvg={(el) =>
                                el
                                    ? knobs.registerKnob(output.key, 'pan', el)
                                    : knobs.unregisterKnob(output.key, 'pan')
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
                                    onToggleMute(output.key, isCodeOnly(e))
                                }
                                onContextMenu={(e) => {
                                    if (e.ctrlKey) {
                                        e.preventDefault();
                                        e.stopPropagation();
                                        onToggleMute(output.key, true);
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
                                        onToggleSolo(output.key, isCodeOnly(e))
                                    }
                                    onContextMenu={(e) => {
                                        if (e.ctrlKey) {
                                            e.preventDefault();
                                            e.stopPropagation();
                                            onToggleSolo(output.key, true);
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
                                          onGainRevert(output.key);
                                      } else {
                                          onGainReset(output.key);
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

export function VuMeterPanel({
    outputs,
    onToggleMute,
    onToggleSolo,
    ghosts,
    onGainChange,
    onGainCommit,
    onGainReset,
    onGainRevert,
    onPeakReset,
    onCanvasResized,
    registerCanvas,
    unregisterCanvas,
    registerReadout,
    unregisterReadout,
    onKnobChange,
    onKnobCommit,
    registerKnob,
    unregisterKnob,
    onJump,
}: VuMeterPanelProps) {
    const [jumpArmed, setJumpArmed] = useJumpModifier();
    const channelOutputs = outputs.filter((o) => !o.main);
    const trackOutputs = channelOutputs.filter((o) => !o.isBus);
    const busOutputs = channelOutputs.filter((o) => o.isBus);
    const mainOutput = outputs.find((o) => o.main) ?? null;
    const folded = useSyncExternalStore(
        foldStore.subscribe,
        () => foldStore.folded,
    );
    // Each group's members sit beside it; a track naming a group that is
    // not there stands on its own.
    const byKey = new Map(channelOutputs.map((o) => [o.key, o]));
    const parentOf = (o: VuMeterDef) =>
        o.group != null && byKey.get(o.group)?.isGroup ? o.group : null;
    const membersOf = (key: string | null) =>
        trackOutputs.filter((o) => parentOf(o) === key);
    const gates = trackGates(channelOutputs);
    const audible = (o: VuMeterDef): boolean => {
        const parent = parentOf(o);
        return (
            gates.get(o.key) === true &&
            (parent === null || audible(byKey.get(parent)!))
        );
    };
    // Numbered in the order shown, hidden members included.
    const indexOf = new Map<string, number>();
    const number = (key: string | null) => {
        for (const o of membersOf(key)) {
            indexOf.set(o.key, indexOf.size + 1);
            number(o.key);
        }
    };
    number(null);
    const panelRef = useRef<HTMLDivElement>(null);
    // A resize re-renders the strips only when it crosses a threshold.
    const [showPan, setShowPan] = useState(false);
    const [showReadout, setShowReadout] = useState(false);
    /** How many rows of send dials fit above a strip that shows its pan. */
    const [sendRowsFit, setSendRowsFit] = useState(0);
    // Measured before the first paint, so the strips open with their knobs.
    useLayoutEffect(() => {
        const panel = panelRef.current;
        if (!panel) return;
        const fit = (height: number) => {
            setShowPan(height >= PAN_MIN_HEIGHT);
            setShowReadout(height >= READOUT_MIN_HEIGHT);
            setSendRowsFit(
                Math.max(
                    0,
                    Math.floor((height - SENDS_MIN_HEIGHT) / SEND_ROW_HEIGHT),
                ),
            );
        };
        fit(panel.getBoundingClientRect().height);
        const observer = new ResizeObserver(([entry]) =>
            fit(entry.borderBoxSize[0].blockSize),
        );
        observer.observe(panel);
        return () => observer.disconnect();
    }, []);

    // The sends show whole or not at all, like a DAW mixer's send section.
    const allSendBuses = busOutputs.map((o, i) => ({
        key: o.key,
        label: o.label ?? o.key,
        letter: busLetter(i),
    }));
    const sendBuses =
        allSendBuses.length > 0 &&
        Math.ceil(allSendBuses.length / SENDS_PER_ROW) <= sendRowsFit
            ? allSendBuses
            : null;

    const meterProps = {
        onCanvasResized,
        onGainChange,
        onGainCommit,
        onGainReset,
        onGainRevert,
        onPeakReset,
        onKnobChange,
        onJump,
        onKnobCommit,
        onToggleMute,
        onToggleSolo,
        registerCanvas,
        registerKnob,
        registerReadout,
        sendBuses,
        showPan,
        showReadout,
        unregisterCanvas,
        unregisterKnob,
        unregisterReadout,
    };

    const renderTrack = (output: VuMeterDef): React.ReactNode => {
        const strip = (
            <VuMeter
                key={output.key}
                output={output}
                ghost={ghosts.get(output.key)}
                index={indexOf.get(output.key)!}
                suppressed={!audible(output)}
                fold={
                    output.isGroup
                        ? {
                              folded: folded.has(output.key),
                              onToggle: () => foldStore.toggle(output.key),
                          }
                        : undefined
                }
                {...meterProps}
            />
        );
        if (!output.isGroup) {
            return strip;
        }
        return (
            <div
                key={output.key}
                className="vu-group"
                data-vu-group={output.key}
            >
                {strip}
                {!folded.has(output.key) &&
                    membersOf(output.key).map(renderTrack)}
            </div>
        );
    };

    return (
        <div
            ref={panelRef}
            className={`vu-meter-panel${jumpArmed ? ' jump-armed' : ''}`}
            // Picks up a modifier pressed while another window had focus.
            onPointerMove={(e) => setJumpArmed(e.ctrlKey || e.metaKey)}
        >
            <div className="vu-meter-scroll">
                {channelOutputs.length === 0 ? (
                    <div className="vu-meter-empty">
                        no tracks — label a line to hear it: lead: $saw('c3')
                    </div>
                ) : (
                    membersOf(null).map(renderTrack)
                )}
            </div>
            {busOutputs.length > 0 && (
                <div className="vu-meter-buses">
                    {busOutputs.map((output, i) => (
                        <VuMeter
                            key={output.key}
                            output={output}
                            ghost={ghosts.get(output.key)}
                            index={busLetter(i)}
                            suppressed={!audible(output)}
                            {...meterProps}
                        />
                    ))}
                </div>
            )}
            {mainOutput && (
                <VuMeter
                    key={mainOutput.key}
                    output={mainOutput}
                    ghost={ghosts.get(mainOutput.key)}
                    index={0}
                    suppressed={false}
                    {...meterProps}
                />
            )}
        </div>
    );
}
