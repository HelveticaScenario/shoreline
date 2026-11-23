import { useEffect, useRef, useState } from 'react';
import type { QueuedTrigger } from '@modular/core';
import type { editor } from 'monaco-editor';
import type {
    DSLExecuteResult,
    TransportSnapshot,
} from '../../../shared/ipcTypes';
import { setActiveInterpolationResolutions } from '../../../shared/dsl/spanTypes';
import { applyRuntimeResolutions } from '../../components/monaco/outputResolutionHints';
import electronAPI from '../../electronAPI';
import type { EqPanelController } from '../../eq/eqPanelController';
import type { MixerController } from '../../mixer/MixerController';
import type { EditorBuffer } from '../../types/editor';
import { getErrorMessage } from '../../utils/errorUtils';
import { getBufferId } from '../buffers';
import {
    NO_PATCH_UI,
    type PatchUi,
    applyInterpolationResolutions,
    buildPatchUi,
    disposePatchUi,
    releaseRunningPatchUi,
} from '../patchUi';
import type { PatchErrors } from './usePatchErrors';

/** The buffer the running patch came from: its path identity
 *  (`getBufferId`), which every consumer compares against the active
 *  buffer's, and its stable id, which survives saves and renames. */
interface RunningBuffer {
    bufferId: string | null;
    sourceId: string | null;
}

const NOT_RUNNING: RunningBuffer = { bufferId: null, sourceId: null };

/**
 * Runs patches and owns what the running one puts on screen. An immediate
 * run shows its UI at once; a run queued for the next bar or beat shows it
 * when the audio thread applies the update, and never if it is cancelled.
 */
export function usePatchRunner({
    buffers,
    activeBufferId,
    activeSourceId,
    getCode,
    editorInstance,
    eqController,
    mixer,
    errors,
}: {
    buffers: EditorBuffer[];
    activeBufferId: string | undefined;
    /** Stable id of the active buffer, sent as the patch's source id so
     *  reconciliation and clock reset key on the buffer, not its path. */
    activeSourceId: string | undefined;
    /** The active buffer's text as the editor has it. */
    getCode: () => string;
    editorInstance: editor.IStandaloneCodeEditor | null;
    eqController: EqPanelController;
    mixer: MixerController;
    errors: PatchErrors;
}) {
    const [isClockRunning, setIsClockRunning] = useState(true);
    const [running, setRunning] = useState(NOT_RUNNING);
    const [patchUi, setPatchUi] = useState(NO_PATCH_UI);
    /** The UI on screen, whose editor decorations are released when the
     *  next one replaces it. */
    const shownRef = useRef(NO_PATCH_UI);
    /** The UI of a queued update the audio thread has yet to apply, and the
     *  running buffer to point back at if the update is cancelled. */
    const pendingRef = useRef<{
        updateId: number;
        ui: PatchUi;
        previousRunning: RunningBuffer;
    } | null>(null);
    const lastResultRef = useRef<DSLExecuteResult | null>(null);
    const runningRef = useRef(running);
    useEffect(() => {
        runningRef.current = running;
    }, [running]);

    // Saving an untitled buffer or renaming a file changes the running
    // buffer's path identity, which follows it here. An untitled buffer's
    // path identity is its id, which a new untitled buffer reuses, so once
    // the running one closes no buffer holds the running patch; a closed
    // file keeps its path, and shows as running when it is opened again.
    if (running.sourceId !== null) {
        const buffer = buffers.find((b) => b.id === running.sourceId);
        if (buffer && getBufferId(buffer) !== running.bufferId) {
            setRunning({ ...running, bufferId: getBufferId(buffer) });
        } else if (!buffer && running.bufferId === running.sourceId) {
            setRunning(NOT_RUNNING);
        }
    }

    const show = (ui: PatchUi) => {
        eqController.commit(ui.eqPanels);
        mixer.commit(ui.vuOutputs, ui.vuAnchors, ui.sourceId);
        releaseRunningPatchUi(shownRef.current);
        shownRef.current = ui;
        setPatchUi(ui);
        if (ui.interpolationResolutions) {
            setActiveInterpolationResolutions(ui.interpolationResolutions);
        }
    };

    const run = async (trigger?: QueuedTrigger) => {
        if (!activeBufferId) {
            return;
        }
        const code = getCode();
        const sourceId = activeSourceId ?? null;
        const result = await electronAPI.executeDSL(
            code,
            sourceId ?? undefined,
            trigger,
        );
        lastResultRef.current = result;
        if (!result.success) {
            applyInterpolationResolutions(result);
            errors.showFailure(result);
            return;
        }

        const previousRunning = runningRef.current;
        setIsClockRunning(true);
        setRunning({ bufferId: activeBufferId, sourceId });
        errors.clear();

        const model = editorInstance?.getModel();
        if (model) {
            void applyRuntimeResolutions(
                model,
                code,
                result.outputResolutions ?? [],
            );
        }
        const ui = buildPatchUi(
            result,
            editorInstance,
            eqController,
            activeBufferId,
            sourceId,
        );
        // A queued update that was never applied gives way to this one.
        disposePatchUi(pendingRef.current?.ui ?? null);
        const queued = trigger === 'NextBar' || trigger === 'NextBeat';
        if (queued && result.updateId != null) {
            // The running patch's decorations stay until the update
            // applies, so its view zones keep working meanwhile.
            pendingRef.current = {
                previousRunning,
                ui,
                updateId: result.updateId,
            };
        } else {
            pendingRef.current = null;
            show(ui);
        }
    };

    const submit = (trigger?: QueuedTrigger) =>
        run(trigger).catch((err) => {
            errors.fail(getErrorMessage(err, 'Unknown error'));
        });

    /** Show or drop the queued update's UI once `transport` reports the
     *  audio thread applied or cancelled it. */
    const settleQueuedUpdate = (transport: TransportSnapshot) => {
        const pending = pendingRef.current;
        if (!pending) {
            return;
        }
        if (transport.lastAppliedUpdateId >= pending.updateId) {
            pendingRef.current = null;
            show(pending.ui);
        } else if (transport.lastCancelledUpdateId === pending.updateId) {
            pendingRef.current = null;
            disposePatchUi(pending.ui);
            setRunning(pending.previousRunning);
        }
    };

    const stop = async () => {
        await electronAPI.synthesizer.stop();
        setIsClockRunning(false);
        setRunning(NOT_RUNNING);
        setPatchUi((prev) => ({ ...prev, sourceId: null }));
    };

    /** Show `value` on the running slider of module `moduleId`. */
    const setSliderValue = (moduleId: string, value: number) =>
        setPatchUi((prev) => ({
            ...prev,
            sliderDefs: prev.sliderDefs.map((s) =>
                s.moduleId === moduleId ? { ...s, value } : s,
            ),
        }));

    const setButtonValue = (moduleId: string, value: boolean) =>
        setPatchUi((prev) => ({
            ...prev,
            buttonDefs: prev.buttonDefs.map((b) =>
                b.moduleId === moduleId ? { ...b, value } : b,
            ),
        }));

    return {
        getLastResult: () => lastResultRef.current,
        isClockRunning,
        patchUi,
        runningBufferId: running.bufferId,
        setButtonValue,
        setSliderValue,
        settleQueuedUpdate,
        stop,
        submit,
    };
}
