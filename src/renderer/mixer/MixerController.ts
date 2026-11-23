import type { VuMeterFrame } from '@modular/core';
import type { editor } from 'monaco-editor';
import type { VuMeterDef } from '../../shared/dsl/vuMeterTypes';
import type { ModelAnchors } from '../app/modelAnchors';
import type { KnobId } from '../components/VuKnob';
import { changeGain, commitGain, resetGain, revertGain } from './mixerGain';
import { changeKnob, commitKnob } from './mixerKnobs';
import { MixerMeters } from './mixerMeters';
import type { MixerParts } from './mixerParts';
import { MixerSourceEdits } from './mixerSourceEdits';
import { MixerStore } from './mixerStore';
import { toggleTrackState } from './mixerTrackState';

/**
 * The mixer of the running patch: its tracks' state, the gestures that set
 * them, and the meters. Every control follows one contract: a plain gesture
 * changes the running audio and the DSL source together, and a code-only
 * (Ctrl/Cmd) gesture changes the source alone, leaving a ghost of the
 * code-side value until a patch update applies it.
 */
export class MixerController {
    readonly store = new MixerStore();
    readonly meters = new MixerMeters(this.store);
    private readonly edits = new MixerSourceEdits(this.store);
    private readonly parts: MixerParts = {
        edits: this.edits,
        meters: this.meters,
        store: this.store,
    };

    attachEditor(instance: editor.IStandaloneCodeEditor | null): void {
        this.edits.attachEditor(instance);
    }

    /** The buffer the editor shows: gestures reach the source only while it
     *  is the one the tracks came from. */
    setVisibleBuffer(bufferId: string | undefined): void {
        this.edits.setVisibleBuffer(bufferId);
    }

    /** Show an applied patch's tracks, built from buffer `sourceId` with
     *  `anchors` on their labels. */
    commit(
        outputs: VuMeterDef[],
        anchors: ModelAnchors | null,
        sourceId: string | null,
    ): void {
        this.edits.commit(anchors, sourceId);
        this.store.commit(outputs);
    }

    /** Offset of a track's label in the editor's source, or null while the
     *  editor shows another buffer than the tracks' own. */
    labelOffset(key: string): number | null {
        return this.edits.labelOffset(key);
    }

    /** Stable id of the buffer the tracks were built from. */
    get sourceId(): string | null {
        return this.edits.source;
    }

    drawFrames(frames: readonly VuMeterFrame[]): void {
        this.meters.drawFrames(frames);
    }

    toggle(key: string, prop: 'mute' | 'solo', codeOnly: boolean): void {
        toggleTrackState(this.parts, key, prop, codeOnly);
    }

    changeGain(key: string, db: number, codeOnly: boolean): void {
        changeGain(this.parts, key, db, codeOnly);
    }

    commitGain(key: string, codeOnly: boolean): void {
        commitGain(this.parts, key, codeOnly);
    }

    resetGain(key: string): void {
        resetGain(this.parts, key);
    }

    revertGain(key: string): void {
        revertGain(this.parts, key);
    }

    changeKnob(
        key: string,
        id: KnobId,
        value: number,
        codeOnly: boolean,
    ): void {
        changeKnob(this.parts, key, id, value, codeOnly);
    }

    commitKnob(
        key: string,
        id: KnobId,
        value: number,
        codeOnly: boolean,
    ): void {
        commitKnob(this.parts, key, id, value, codeOnly);
    }
}
