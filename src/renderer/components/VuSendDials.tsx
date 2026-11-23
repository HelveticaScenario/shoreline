import React from 'react';
import { SEND_LEVEL_RANGE } from '../../shared/dsl/controlRanges';
import {
    sendGhostKey,
    type VuMeterGhost,
    type VuSendDef,
} from '../../shared/dsl/vuMeterTypes';
import type { MixerController } from '../mixer/MixerController';
import { Knob, type KnobRange } from './VuKnob';

/** Height of one row of send dials, letter included. */
export const SEND_ROW_HEIGHT = 32;
/** Send dials per row of a strip. */
export const SENDS_PER_ROW = 3;

const SEND_RANGE: KnobRange = { ...SEND_LEVEL_RANGE, origin: 0 };

/** A bus a strip can send to: its key, the letter it shows, its name. */
export interface SendBus {
    key: string;
    letter: string;
    label: string;
}

/** A send dial's tooltip. */
function sendTitle(bus: string, send: VuSendDef): string {
    return send.locked
        ? `Send to ${bus} (set in code)`
        : `Send to ${bus}: ${send.value} (drag; ⌃/⌘ drag: edit code only; right-click: off; ⌃/⌘ right-click: revert)`;
}

/**
 * A strip's send dials, one per bus in the order the buses are shown, each
 * a {@link Knob} whose release at 0 removes the send. A strip with no sends
 * (a bus, the main) keeps the section's space empty so every strip's meter
 * lines up.
 */
export function VuSendDials({
    trackKey,
    buses,
    sends,
    ghost,
    mixer,
}: {
    trackKey: string;
    buses: SendBus[];
    sends: VuSendDef[] | undefined;
    ghost: VuMeterGhost | undefined;
    mixer: MixerController;
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
                    const id = sendGhostKey(bus.key);
                    return (
                        // Remounted when its lock changes, which drops what
                        // the meter loop painted onto it.
                        <div
                            key={`${bus.key}:${send.locked}`}
                            className="vu-send"
                        >
                            <span className="vu-send-letter">{bus.letter}</span>
                            <Knob
                                className="vu-send-knob"
                                value={send.value ?? 0}
                                ghost={ghost?.[id]}
                                range={SEND_RANGE}
                                step={0.01}
                                resetValue={0}
                                locked={send.locked}
                                title={sendTitle(bus.label, send)}
                                onChange={(value, codeOnly) =>
                                    mixer.changeKnob(
                                        trackKey,
                                        id,
                                        value,
                                        codeOnly,
                                    )
                                }
                                onCommit={(value, codeOnly) =>
                                    mixer.commitKnob(
                                        trackKey,
                                        id,
                                        value,
                                        codeOnly,
                                    )
                                }
                                registerSvg={(el) =>
                                    el
                                        ? mixer.meters.registerKnob(
                                              trackKey,
                                              id,
                                              el,
                                          )
                                        : mixer.meters.unregisterKnob(
                                              trackKey,
                                              id,
                                          )
                                }
                            />
                        </div>
                    );
                })}
        </div>
    );
}
