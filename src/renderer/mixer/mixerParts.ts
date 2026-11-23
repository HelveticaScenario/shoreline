import type { MixerMeters } from './mixerMeters';
import type { MixerSourceEdits } from './mixerSourceEdits';
import type { MixerStore } from './mixerStore';

/** The engine calls the mixer's gestures make. */
export interface MixerEngine {
    setModuleParam: (
        moduleId: string,
        moduleType: string,
        params: object,
    ) => Promise<unknown>;
}

/** What a mixer gesture works on. */
export interface MixerParts {
    store: MixerStore;
    edits: MixerSourceEdits;
    meters: MixerMeters;
    engine: MixerEngine;
}
