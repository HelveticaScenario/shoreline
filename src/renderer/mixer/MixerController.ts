import type { VuMeterDef } from '../../shared/dsl/vuMeterTypes';
import type { ModelAnchors } from '../app/modelAnchors';
import { MixerGain } from './mixerGain';
import { MixerKnobs } from './mixerKnobs';
import { MixerMeters } from './mixerMeters';
import type { MixerEngine } from './mixerParts';
import { MixerSourceEdits } from './mixerSourceEdits';
import { MixerStore } from './mixerStore';
import { MixerTrackState } from './mixerTrackState';

/**
 * The mixer of the running patch, built around one store and one engine.
 * Its parts are its API: `store` holds the tracks' state, `meters` paints
 * them, `edits` places them in the source, and `gain`, `knobs` and `tracks`
 * are the gestures that set them. Every gesture follows one contract: a
 * plain one changes the running audio and the DSL source together, and a
 * code-only (Ctrl/Cmd) one changes the source alone, leaving a ghost of the
 * code-side value until a patch update applies it.
 */
export class MixerController {
    readonly store = new MixerStore();
    readonly meters = new MixerMeters(this.store);
    readonly edits = new MixerSourceEdits(this.store);
    readonly gain: MixerGain;
    readonly knobs: MixerKnobs;
    readonly tracks: MixerTrackState;

    constructor(engine: MixerEngine) {
        const parts = {
            edits: this.edits,
            engine,
            meters: this.meters,
            store: this.store,
        };
        this.gain = new MixerGain(parts);
        this.knobs = new MixerKnobs(parts);
        this.tracks = new MixerTrackState(parts);
    }

    /** Show an applied patch's tracks, built from buffer `sourceId` with
     *  `anchors` on their labels. The anchors are index-aligned with
     *  `outputs`, so the two always change together, here. */
    commit(
        outputs: VuMeterDef[],
        anchors: ModelAnchors | null,
        sourceId: string | null,
    ): void {
        this.edits.setSource(anchors, sourceId);
        this.store.commit(outputs);
    }
}
