import { type MutableRefObject, useCallback, useRef } from 'react';
import { sendLevelParams } from '../../../shared/dsl/controlRanges';
import type {
    VuMeterDef,
    VuMeterGhost,
} from '../../../shared/dsl/vuMeterTypes';
import electronAPI from '../../electronAPI';
import { paintSendDial } from '../../components/VuSendDials';
import { computeTrackSendEdit } from '../../dsl/trackSendEdit';
import type { TrackSourceEdit } from '../../dsl/trackSourceEdit';

type SourceEdit = (
    source: string,
    anchor: number,
    name: string,
) => TrackSourceEdit | null;

/**
 * The mixer's send dials: a drag drives the send's lifted level live and
 * writes it into the track's `sends` at the throttled rate the other
 * controls use; the release writes the final level, removing the send at 0.
 * A code-only (Ctrl/Cmd) gesture writes the source alone and leaves a ghost
 * of the code-side level on the dial until a patch update applies it.
 */
export function useVuSendControls({
    vuOutputsRef,
    setVuOutputs,
    vuGhostsRef,
    setVuGhostProp,
    applyVuSourceEdit,
    scheduleVuEdit,
    cancelVuEdit,
}: {
    vuOutputsRef: MutableRefObject<VuMeterDef[]>;
    setVuOutputs: (outputs: VuMeterDef[]) => void;
    vuGhostsRef: MutableRefObject<Map<string, VuMeterGhost>>;
    setVuGhostProp: (
        key: string,
        prop: 'sends',
        value: VuMeterGhost['sends'] | undefined,
    ) => void;
    applyVuSourceEdit: (key: string, makeEdit: SourceEdit) => boolean;
    scheduleVuEdit: (timerKey: string, write: () => void) => void;
    cancelVuEdit: (timerKey: string) => void;
}) {
    /** Locked dials' SVGs, painted live from the meter loop. */
    const sendDialsRef = useRef(new Map<string, Map<string, SVGSVGElement>>());

    const writeSendEdit = useCallback(
        (key: string, bus: string, level: number | null) => {
            const outputs = vuOutputsRef.current;
            const output = outputs.find((o) => o.key === key);
            if (!output) {
                return;
            }
            const buses = new Set(
                outputs.filter((o) => o.isBus).map((o) => o.key),
            );
            applyVuSourceEdit(key, (source, anchor, name) =>
                computeTrackSendEdit(
                    source,
                    anchor,
                    name,
                    bus,
                    level,
                    output.fromTrack === true,
                    buses,
                ),
            );
        },
        [applyVuSourceEdit, vuOutputsRef],
    );

    /** The send a dial controls: one whose level it can set. */
    const dialSend = useCallback(
        (key: string, bus: string) => {
            const output = vuOutputsRef.current.find((o) => o.key === key);
            const send = output?.sends?.find((s) => s.bus === bus);
            return output && send?.moduleId ? { output, send } : null;
        },
        [vuOutputsRef],
    );

    /** Set or clear (`level` undefined) the code-side level of one send. */
    const setSendGhost = useCallback(
        (key: string, bus: string, level: number | undefined) => {
            const sends = { ...vuGhostsRef.current.get(key)?.sends };
            if (level === undefined) {
                if (!(bus in sends)) {
                    return;
                }
                delete sends[bus];
            } else {
                sends[bus] = level;
            }
            setVuGhostProp(
                key,
                'sends',
                Object.keys(sends).length > 0 ? sends : undefined,
            );
        },
        [setVuGhostProp, vuGhostsRef],
    );

    /** Set the send's live level, mirrored into the meter state. Returns
     *  whether the send is one a dial controls. */
    const setLiveLevel = useCallback(
        (key: string, bus: string, level: number): boolean => {
            const send = dialSend(key, bus)?.send;
            if (!send?.moduleId) {
                return false;
            }
            const outputs = vuOutputsRef.current;
            if (send.level !== level) {
                void electronAPI.synthesizer.setModuleParam(
                    send.moduleId,
                    '_smoothControl',
                    sendLevelParams(level),
                );
                const next = outputs.map((o) =>
                    o.key === key
                        ? {
                              ...o,
                              sends: o.sends?.map((s) =>
                                  s.bus === bus ? { ...s, level } : s,
                              ),
                          }
                        : o,
                );
                vuOutputsRef.current = next;
                setVuOutputs(next);
            }
            return true;
        },
        [dialSend, setVuOutputs, vuOutputsRef],
    );

    /** A code-only gesture moves the ghost alone; a code-side level equal
     *  to the audio's drops it. Returns whether the dial is code-editable. */
    const setCodeLevel = useCallback(
        (key: string, bus: string, level: number): boolean => {
            const controlled = dialSend(key, bus);
            if (!controlled?.output.sourceLocation) {
                return false;
            }
            setSendGhost(
                key,
                bus,
                level === controlled.send.level ? undefined : level,
            );
            return true;
        },
        [dialSend, setSendGhost],
    );

    const handleSendChange = useCallback(
        (key: string, bus: string, level: number, codeOnly: boolean) => {
            if (codeOnly) {
                if (!setCodeLevel(key, bus, level)) {
                    return;
                }
            } else if (setLiveLevel(key, bus, level)) {
                setSendGhost(key, bus, undefined);
            } else {
                return;
            }
            scheduleVuEdit(`${key}:send:${bus}`, () =>
                writeSendEdit(key, bus, level),
            );
        },
        [
            scheduleVuEdit,
            setCodeLevel,
            setLiveLevel,
            setSendGhost,
            writeSendEdit,
        ],
    );

    const handleSendCommit = useCallback(
        (key: string, bus: string, level: number, codeOnly: boolean) => {
            if (codeOnly) {
                if (!setCodeLevel(key, bus, level)) {
                    return;
                }
            } else if (setLiveLevel(key, bus, level)) {
                setSendGhost(key, bus, undefined);
            } else {
                return;
            }
            cancelVuEdit(`${key}:send:${bus}`);
            writeSendEdit(key, bus, level === 0 ? null : level);
        },
        [cancelVuEdit, setCodeLevel, setLiveLevel, setSendGhost, writeSendEdit],
    );

    const registerSendDial = useCallback(
        (key: string, bus: string, el: SVGSVGElement) => {
            const dials = sendDialsRef.current;
            const forTrack = dials.get(key) ?? new Map();
            forTrack.set(bus, el);
            dials.set(key, forTrack);
        },
        [],
    );

    const unregisterSendDial = useCallback((key: string, bus: string) => {
        const forTrack = sendDialsRef.current.get(key);
        forTrack?.delete(bus);
        if (forTrack?.size === 0) {
            sendDialsRef.current.delete(key);
        }
    }, []);

    /** Paint `output`'s locked dials from a meter frame's send levels. */
    const paintSendFrame = useCallback(
        (output: VuMeterDef, levels: readonly number[]) => {
            const dials = sendDialsRef.current.get(output.key);
            if (!dials) {
                return;
            }
            for (const send of output.sends ?? []) {
                const dial = dials.get(send.bus);
                const level =
                    send.sourceIndex === undefined
                        ? undefined
                        : levels[send.sourceIndex];
                if (dial && level !== undefined) {
                    paintSendDial(dial, level);
                }
            }
        },
        [],
    );

    return {
        handleSendChange,
        handleSendCommit,
        paintSendFrame,
        registerSendDial,
        unregisterSendDial,
    };
}
