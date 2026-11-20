import { type MutableRefObject, useCallback, useRef } from 'react';
import {
    type ControlParams,
    outPanParams,
    sendLevelParams,
} from '../../../shared/dsl/controlRanges';
import type { VuControl, VuMeterDef } from '../../../shared/dsl/vuMeterTypes';
import { type KnobId, paintKnob } from '../../components/VuKnob';
import electronAPI from '../../electronAPI';
import { computeTrackSendEdit } from '../../dsl/trackSendEdit';
import {
    computeTrackOptionEdit,
    type TrackSourceEdit,
} from '../../dsl/trackSourceEdit';

type SourceEdit = (
    source: string,
    anchor: number,
    name: string,
) => TrackSourceEdit | null;

/** The bus a send knob's id names. */
const busOf = (id: KnobId): string => id.slice('send:'.length);

/** The control knob `id` sets on `output`. */
function controlOf(output: VuMeterDef, id: KnobId): VuControl | null {
    if (id === 'pan') {
        return output.pan;
    }
    const bus = busOf(id);
    return output.sends?.find((s) => s.bus === bus) ?? null;
}

/** `output` with knob `id`'s control showing `value`. */
function withValue(output: VuMeterDef, id: KnobId, value: number): VuMeterDef {
    if (id === 'pan') {
        return { ...output, pan: output.pan && { ...output.pan, value } };
    }
    const bus = busOf(id);
    return {
        ...output,
        sends: output.sends?.map((s) => (s.bus === bus ? { ...s, value } : s)),
    };
}

const liveParams = (id: KnobId, value: number): ControlParams =>
    id === 'pan' ? outPanParams(value) : sendLevelParams(value);

/**
 * The mixer's knobs — pan and the send dials. A drag drives the knob's
 * lifted control live and writes the value into the track's `.track()` at
 * the throttled rate the other controls use; the release writes the final
 * value. Pan at its centre, and a send released at 0, leave the source.
 * A code-only (Ctrl/Cmd) gesture writes the source alone and leaves a ghost
 * of the code-side value on the knob until a patch update applies it.
 */
export function useVuKnobControls({
    vuOutputsRef,
    setVuOutputs,
    setVuGhostProp,
    applyVuSourceEdit,
    scheduleVuEdit,
    cancelVuEdit,
}: {
    vuOutputsRef: MutableRefObject<VuMeterDef[]>;
    setVuOutputs: (outputs: VuMeterDef[]) => void;
    setVuGhostProp: (
        key: string,
        prop: KnobId,
        value: number | undefined,
    ) => void;
    applyVuSourceEdit: (key: string, makeEdit: SourceEdit) => boolean;
    scheduleVuEdit: (timerKey: string, write: () => void) => void;
    cancelVuEdit: (timerKey: string) => void;
}) {
    /** Locked knobs' SVGs by track key, then knob id, painted live from the
     *  meter loop. */
    const knobsRef = useRef(new Map<string, Map<KnobId, SVGSVGElement>>());

    /** The track and control knob `id` drives, when it is one it sets. */
    const knobTarget = useCallback(
        (key: string, id: KnobId) => {
            const output = vuOutputsRef.current.find((o) => o.key === key);
            const control = output && controlOf(output, id);
            return output && control?.moduleId ? { control, output } : null;
        },
        [vuOutputsRef],
    );

    /** Write `value` into the source; a release (`final`) at 0 removes a
     *  send, and pan at its centre is always left out. */
    const writeEdit = useCallback(
        (key: string, id: KnobId, value: number, final: boolean) => {
            const outputs = vuOutputsRef.current;
            const output = outputs.find((o) => o.key === key);
            if (!output) {
                return;
            }
            const fromTrack = output.fromTrack === true;
            if (id === 'pan') {
                applyVuSourceEdit(key, (source, anchor, name) =>
                    computeTrackOptionEdit(
                        source,
                        anchor,
                        name,
                        'pan',
                        value === 0 ? null : value,
                        fromTrack,
                    ),
                );
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
                    busOf(id),
                    final && value === 0 ? null : value,
                    fromTrack,
                    buses,
                ),
            );
        },
        [applyVuSourceEdit, vuOutputsRef],
    );

    /**
     * Apply a knob gesture to the audio or, code-only, to the ghost alone; a
     * code-side value equal to the audio's drops the ghost. Returns whether
     * the knob is one the gesture sets.
     */
    const applyGesture = useCallback(
        (key: string, id: KnobId, value: number, codeOnly: boolean) => {
            const target = knobTarget(key, id);
            if (!target) {
                return false;
            }
            if (codeOnly) {
                if (!target.output.sourceLocation) {
                    return false;
                }
                setVuGhostProp(
                    key,
                    id,
                    value === target.control.value ? undefined : value,
                );
                return true;
            }
            if (target.control.value !== value) {
                void electronAPI.synthesizer.setModuleParam(
                    target.control.moduleId!,
                    '_smoothControl',
                    liveParams(id, value),
                );
                const next = vuOutputsRef.current.map((o) =>
                    o.key === key ? withValue(o, id, value) : o,
                );
                vuOutputsRef.current = next;
                setVuOutputs(next);
            }
            setVuGhostProp(key, id, undefined);
            return true;
        },
        [knobTarget, setVuGhostProp, setVuOutputs, vuOutputsRef],
    );

    const handleKnobChange = useCallback(
        (key: string, id: KnobId, value: number, codeOnly: boolean) => {
            if (applyGesture(key, id, value, codeOnly)) {
                scheduleVuEdit(`${key}:${id}`, () =>
                    writeEdit(key, id, value, false),
                );
            }
        },
        [applyGesture, scheduleVuEdit, writeEdit],
    );

    const handleKnobCommit = useCallback(
        (key: string, id: KnobId, value: number, codeOnly: boolean) => {
            if (applyGesture(key, id, value, codeOnly)) {
                cancelVuEdit(`${key}:${id}`);
                writeEdit(key, id, value, true);
            }
        },
        [applyGesture, cancelVuEdit, writeEdit],
    );

    const registerKnob = useCallback(
        (key: string, id: KnobId, el: SVGSVGElement) => {
            const knobs = knobsRef.current;
            const forTrack = knobs.get(key) ?? new Map();
            forTrack.set(id, el);
            knobs.set(key, forTrack);
        },
        [],
    );

    const unregisterKnob = useCallback((key: string, id: KnobId) => {
        const forTrack = knobsRef.current.get(key);
        forTrack?.delete(id);
        if (forTrack?.size === 0) {
            knobsRef.current.delete(key);
        }
    }, []);

    /** Paint `output`'s locked knobs from a meter frame's control values. */
    const paintKnobFrame = useCallback(
        (output: VuMeterDef, values: readonly number[]) => {
            const knobs = knobsRef.current.get(output.key);
            if (!knobs) {
                return;
            }
            for (const [id, svg] of knobs) {
                const index = controlOf(output, id)?.sourceIndex;
                const value = index === undefined ? undefined : values[index];
                if (value !== undefined) {
                    paintKnob(svg, value);
                }
            }
        },
        [],
    );

    return {
        handleKnobChange,
        handleKnobCommit,
        paintKnobFrame,
        registerKnob,
        unregisterKnob,
    };
}
