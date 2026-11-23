import {
    type ControlParams,
    outPanParams,
    sendLevelParams,
} from '../../shared/dsl/controlRanges';
import type { VuMeterDef } from '../../shared/dsl/vuMeterTypes';
import type { KnobId } from '../components/VuKnob';
import { computeTrackSendEdit } from '../dsl/trackSendEdit';
import { computeTrackOptionEdit } from '../dsl/trackSourceEdit';
import electronAPI from '../electronAPI';
import { busOfKnob, knobControl } from './mixerMeters';
import type { MixerParts } from './mixerParts';

/** `output` with knob `id`'s control showing `value`. */
function withValue(output: VuMeterDef, id: KnobId, value: number): VuMeterDef {
    if (id === 'pan') {
        return { ...output, pan: output.pan && { ...output.pan, value } };
    }
    const bus = busOfKnob(id);
    return {
        ...output,
        sends: output.sends?.map((s) => (s.bus === bus ? { ...s, value } : s)),
    };
}

const liveParams = (id: KnobId, value: number): ControlParams =>
    id === 'pan' ? outPanParams(value) : sendLevelParams(value);

const timerKey = (key: string, id: KnobId) => `${key}:${id}`;

/** Write `value` into the source; a release (`final`) at 0 removes a send,
 *  and pan at its centre is always left out. */
function writeKnob(
    { store, edits }: MixerParts,
    key: string,
    id: KnobId,
    value: number,
    final: boolean,
): void {
    const output = store.output(key);
    if (!output) {
        return;
    }
    const fromTrack = output.fromTrack === true;
    if (id === 'pan') {
        edits.apply(key, (source, anchor, name) =>
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
        store.outputs.filter((o) => o.isBus).map((o) => o.key),
    );
    edits.apply(key, (source, anchor, name) =>
        computeTrackSendEdit(
            source,
            anchor,
            name,
            busOfKnob(id),
            final && value === 0 ? null : value,
            fromTrack,
            buses,
        ),
    );
}

/**
 * Apply a knob gesture to the audio or, code-only, to the ghost alone; a
 * code-side value equal to the audio's drops the ghost. Returns whether the
 * knob is one the gesture sets.
 */
function applyGesture(
    { store }: MixerParts,
    key: string,
    id: KnobId,
    value: number,
    codeOnly: boolean,
): boolean {
    const output = store.output(key);
    const control = output && knobControl(output, id);
    if (!output || !control?.moduleId) {
        return false;
    }
    if (codeOnly) {
        if (!output.sourceLocation) {
            return false;
        }
        store.setGhostProp(
            key,
            id,
            value === control.value ? undefined : value,
        );
        return true;
    }
    if (control.value !== value) {
        void electronAPI.synthesizer.setModuleParam(
            control.moduleId,
            '_smoothControl',
            liveParams(id, value),
        );
        store.updateOutput(key, (o) => withValue(o, id, value));
    }
    store.setGhostProp(key, id, undefined);
    return true;
}

/**
 * A drag on a pan knob or send dial: drive the knob's lifted control live
 * and write the value into the track's `.track()` at the throttled rate. A
 * code-only (Ctrl/Cmd) gesture writes the source alone and leaves a ghost of
 * the code-side value on the knob until a patch update applies it.
 */
export function changeKnob(
    parts: MixerParts,
    key: string,
    id: KnobId,
    value: number,
    codeOnly: boolean,
): void {
    if (applyGesture(parts, key, id, value, codeOnly)) {
        parts.edits.schedule(timerKey(key, id), () =>
            writeKnob(parts, key, id, value, false),
        );
    }
}

/** The release of a knob drag, or a reset or revert: write the final value. */
export function commitKnob(
    parts: MixerParts,
    key: string,
    id: KnobId,
    value: number,
    codeOnly: boolean,
): void {
    if (applyGesture(parts, key, id, value, codeOnly)) {
        parts.edits.cancel(timerKey(key, id));
        writeKnob(parts, key, id, value, true);
    }
}
