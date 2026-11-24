import React, { useLayoutEffect, useRef, useState } from 'react';
import type { VuMeterDef } from '../../shared/dsl/vuMeterTypes';
import { trackGates } from '../../shared/dsl/trackGates';
import type { MixerController } from '../mixer/MixerController';
import { useMixerSnapshot } from '../mixer/mixerStore';
import { useJumpModifier } from './controlPanel/controlChrome';
import { VuMeter } from './VuMeter';
import { SEND_ROW_HEIGHT, SENDS_PER_ROW } from './VuSendDials';
import './VuMeterPanel.css';

/** Strip height at which the pan knob appears. */
const PAN_MIN_HEIGHT = 150;
/** Strip height at which the peak-readout pill appears. */
const READOUT_MIN_HEIGHT = 180;
/** Panel height left for a strip's pan, mute and solo once its send dials
 *  are shown; each row of dials needs this plus a row's height, so the
 *  sends give way before the pan knob does. */
const SENDS_MIN_HEIGHT = 140;

/** The letter a bus shows on its activator: A, B, … Z, AA, AB, … */
function busLetter(index: number): string {
    const letter = String.fromCharCode(65 + (index % 26));
    return index < 26 ? letter : busLetter(Math.floor(index / 26) - 1) + letter;
}

export function VuMeterPanel({
    mixer,
    onJump,
}: {
    mixer: MixerController;
    /** Ctrl/Cmd-click on a strip's name: move the cursor to its label. */
    onJump: (key: string) => void;
}) {
    const jump = useJumpModifier();
    const { outputs, ghosts, folded } = useMixerSnapshot(mixer.store);
    const channelOutputs = outputs.filter((o) => !o.main);
    const trackOutputs = channelOutputs.filter((o) => !o.isBus);
    const busOutputs = channelOutputs.filter((o) => o.isBus);
    const mainOutput = outputs.find((o) => o.main) ?? null;
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

    const meterProps = { mixer, onJump, sendBuses, showPan, showReadout };

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
                              onToggle: () =>
                                  mixer.store.toggleFold(output.key),
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
            className={`vu-meter-panel${jump.armed ? ' jump-armed' : ''}`}
            onPointerMove={jump.onPointerMove}
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
