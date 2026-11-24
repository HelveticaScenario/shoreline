import type {
    SendGhostKey,
    VuControl,
    VuMeterDef,
} from '../../shared/dsl/vuMeterTypes';

/** A track's knob: its pan, or its send to a bus. */
export type KnobId = 'pan' | SendGhostKey;

/** The bus a send knob's id names. */
export const busOfKnob = (id: KnobId): string => id.slice('send:'.length);

/** The control knob `id` sets on `output`. */
export function knobControl(output: VuMeterDef, id: KnobId): VuControl | null {
    if (id === 'pan') {
        return output.pan;
    }
    const bus = busOfKnob(id);
    return output.sends?.find((s) => s.bus === bus) ?? null;
}
