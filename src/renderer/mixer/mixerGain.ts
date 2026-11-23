import { outGainParams } from '../../shared/dsl/controlRanges';
import {
    DEFAULT_OUTPUT_GAIN,
    UNITY_OUT_GAIN,
    dbToOutGain,
    outGainToDb,
    type VuMeterDef,
} from '../../shared/dsl/vuMeterTypes';
import {
    computeSetMainGainEdit,
    computeTrackOptionEdit,
} from '../dsl/trackSourceEdit';
import electronAPI from '../electronAPI';
import type { MixerParts } from './mixerParts';

/** The output a fader gesture on `key` sets, when its gain is one it can. */
function faderTarget(
    { store }: MixerParts,
    key: string,
): (VuMeterDef & { gain: { moduleId: string } }) | null {
    const output = store.output(key);
    return output?.gain.moduleId
        ? (output as VuMeterDef & { gain: { moduleId: string } })
        : null;
}

/** Write `gain` into the source: the track's `gain` option (removed for
 *  null), or $setMainGain for the master. */
function writeGain(
    { edits }: MixerParts,
    output: VuMeterDef,
    gain: number | null,
): void {
    if (output.main) {
        if (gain !== null) {
            edits.apply(output.key, (source) =>
                computeSetMainGainEdit(source, gain),
            );
        }
        return;
    }
    const fromTrack = output.fromTrack === true;
    edits.apply(output.key, (source, anchor, name) =>
        computeTrackOptionEdit(source, anchor, name, 'gain', gain, fromTrack),
    );
}

/** Drive the gain's lifted control and show `gain` as the audio's value. */
function setLiveGain(
    { store }: MixerParts,
    output: VuMeterDef & { gain: { moduleId: string } },
    gain: number,
): void {
    void electronAPI.synthesizer.setModuleParam(
        output.gain.moduleId,
        '_smoothControl',
        outGainParams(gain),
    );
    store.updateOutput(output.key, (o) => ({
        ...o,
        gain: { ...o.gain, value: gain },
    }));
    store.setGhostProp(output.key, 'gain', undefined);
}

const timerKey = (key: string) => `${key}:gain`;

/**
 * Fader drag on a meter: drive the gain live and repaint the triangle at
 * once (the meter loop only runs while the clock does); the source write is
 * throttled. A code-only drag leaves the audio and the solid triangle alone
 * and moves only the ghost triangle, writing the source at the same rate.
 */
export function changeGain(
    parts: MixerParts,
    key: string,
    db: number,
    codeOnly: boolean,
): void {
    const { store, edits, meters } = parts;
    const output = faderTarget(parts, key);
    if (!output) {
        return;
    }
    // Snap to 0.5 dB steps; the scale floor means silence.
    const snappedDb = Math.round(db * 2) / 2;
    const gain =
        snappedDb <= -60 ? 0 : Number(dbToOutGain(snappedDb).toPrecision(4));
    const write = () => {
        const current = faderTarget(parts, key);
        if (current) {
            writeGain(parts, current, gain);
        }
    };

    if (codeOnly) {
        if (!output.main && !output.sourceLocation) {
            return;
        }
        if (gain === store.ghost(key)?.gain) {
            return;
        }
        store.setGhostProp(
            key,
            'gain',
            gain === output.gain.value ? undefined : gain,
        );
        meters.redraw(key);
        edits.schedule(timerKey(key), write);
        return;
    }

    if (gain === output.gain.value) {
        return;
    }
    setLiveGain(parts, output, gain);
    meters.redraw(key, gain === 0 ? -Infinity : snappedDb);
    edits.schedule(timerKey(key), write);
}

/** Drag released: flush the final gain into the source immediately. */
export function commitGain(
    parts: MixerParts,
    key: string,
    codeOnly: boolean,
): void {
    const { store, edits } = parts;
    const output = store.output(key);
    if (!output) {
        return;
    }
    const gain = codeOnly
        ? (store.ghost(key)?.gain ?? output.gain.value)
        : output.gain.value;
    if (gain === null) {
        return;
    }
    edits.cancel(timerKey(key));
    if (output.gain.moduleId) {
        writeGain(parts, output, gain);
    }
}

/** Revert the source to the gain the audio is running, dropping the ghost;
 *  unity reverts by removing the option. No-op when code and audio agree. */
export function revertGain(parts: MixerParts, key: string): void {
    const { store, edits, meters } = parts;
    const output = faderTarget(parts, key);
    const audioGain = output?.gain.value ?? null;
    if (!output || audioGain === null || store.ghost(key)?.gain === undefined) {
        return;
    }
    edits.cancel(timerKey(key));
    writeGain(
        parts,
        output,
        !output.main && audioGain === UNITY_OUT_GAIN ? null : audioGain,
    );
    store.setGhostProp(key, 'gain', undefined);
    meters.redraw(key);
}

/** Back to the default gain: unity for a track, whose option is removed,
 *  and the main gain default for the master. */
export function resetGain(parts: MixerParts, key: string): void {
    const { edits, meters } = parts;
    edits.cancel(timerKey(key));
    const output = faderTarget(parts, key);
    if (!output) {
        return;
    }
    const resetValue = output.main ? DEFAULT_OUTPUT_GAIN : UNITY_OUT_GAIN;
    writeGain(parts, output, output.main ? resetValue : null);
    setLiveGain(parts, output, resetValue);
    meters.redraw(key, outGainToDb(resetValue));
}
