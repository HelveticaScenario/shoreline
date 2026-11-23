import { gateParams } from '../../shared/dsl/controlRanges';
import { trackGates } from '../../shared/dsl/trackGates';
import type { TrackState } from '../../shared/dsl/trackLabels';
import type { VuMeterDef } from '../../shared/dsl/vuMeterTypes';
import { computeTrackStateEdit } from '../dsl/trackSourceEdit';
import electronAPI from '../electronAPI';
import type { MixerParts } from './mixerParts';

const gatesOf = (outputs: VuMeterDef[]) =>
    trackGates(outputs.filter((o) => o.muteModuleId != null));

/**
 * A click on a track's mute or solo. The two toggle on their own, and a
 * soloed track plays whether or not it is muted. A plain click live-updates
 * every mute gate whose effective value changes (solo reaches across tracks
 * and groups) and rewrites the track's label prefix, with no patch re-eval.
 * A code-only click edits the source alone: the audio keeps running and the
 * button's outer (code) section ghosts until a patch update applies it.
 */
export function toggleTrackState(
    { store, edits }: MixerParts,
    key: string,
    prop: 'mute' | 'solo',
    codeOnly: boolean,
): void {
    const outputs = store.outputs;
    const audio = store.output(key);
    // A bus has no solo.
    if (!audio || (prop === 'solo' && audio.isBus)) {
        return;
    }
    // What the code says, which a code-only click may have set apart from
    // the audio.
    const ghost = store.ghost(key);
    const code: TrackState = {
        mute: ghost?.mute ?? audio.mute,
        solo: ghost?.solo ?? audio.solo,
    };
    const writeState = (state: TrackState) =>
        edits.apply(key, (source, anchor, name) =>
            computeTrackStateEdit(source, anchor, name, state),
        );

    if (codeOnly) {
        const nextCode = { ...code, [prop]: !code[prop] };
        if (writeState(nextCode)) {
            store.setGhostProp(
                key,
                prop,
                nextCode[prop] === audio[prop] ? undefined : nextCode[prop],
            );
        }
        return;
    }

    const next = outputs.map((o) =>
        o.key === key ? { ...o, [prop]: !audio[prop] } : o,
    );
    const before = gatesOf(outputs);
    const after = gatesOf(next);
    for (const output of next) {
        // The main meter has no gate.
        const { muteModuleId } = output;
        if (
            muteModuleId != null &&
            before.get(output.key) !== after.get(output.key)
        ) {
            void electronAPI.synthesizer.setModuleParam(
                muteModuleId,
                '$signal',
                gateParams(after.get(output.key) ? 5 : 0),
            );
        }
    }

    // The other control keeps what the code says.
    writeState({ ...code, [prop]: !audio[prop] });
    // Code and audio agree again for this control.
    store.setGhostProp(key, prop, undefined);
    // From the store, which the source edit may have just updated.
    store.updateOutput(key, (o) => ({ ...o, [prop]: !audio[prop] }));
}
