import type { MixerMeters } from './mixerMeters';
import type { MixerSourceEdits } from './mixerSourceEdits';
import type { MixerStore } from './mixerStore';

/** What a mixer gesture works on. */
export interface MixerParts {
    store: MixerStore;
    edits: MixerSourceEdits;
    meters: MixerMeters;
}
