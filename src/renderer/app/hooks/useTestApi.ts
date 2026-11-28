import { useEffect, useEffectEvent } from 'react';
import electronAPI from '../../electronAPI';
import type { EqPanelController } from '../../eq/eqPanelController';
import { executeCommand } from '../../keybindings/commands';
import { getActiveEditor } from '../../keybindings/dispatch';
import type { MixerController } from '../../mixer/MixerController';

/** Exposes `window.__TEST_API__`, which the E2E tests drive the app through. */
export function useTestApi(app: {
    eqController: EqPanelController;
    mixer: MixerController;
    isEngineRunning: boolean;
    submit: () => void;
    getLastResult: () => unknown;
    openEngineHealth: () => void;
    openModuleProfile: () => void;
    setMixerVisible: (visible: boolean) => void;
}): void {
    const latest = useEffectEvent(() => app);

    useEffect(() => {
        window.__TEST_API__ = {
            executePatch: async () => {
                latest().submit();
            },
            getAudioHealth: () => electronAPI.synthesizer.getHealth(),
            getCursorOffset: () => {
                const editorNow = getActiveEditor();
                const model = editorNow?.getModel();
                const position = editorNow?.getPosition();
                return model && position ? model.getOffsetAt(position) : null;
            },
            getEditorValue: () => getActiveEditor()?.getValue() ?? '',
            getEqPanels: () => latest().eqController.snapshot(),
            getLastPatchResult: () => latest().getLastResult(),
            getScopeData: () => electronAPI.synthesizer.getScopes(),
            getVuMeterData: () => electronAPI.synthesizer.getVuMeters(),
            getVuOutputs: () => latest().mixer.store.outputs,
            isEngineRunning: () => latest().isEngineRunning,
            newUntitledFile: () => executeCommand('shoreline.newFile'),
            openEngineHealth: () => latest().openEngineHealth(),
            openModuleProfile: () => latest().openModuleProfile(),
            setEditorValue: (code: string) => getActiveEditor()?.setValue(code),
            setMixerVisible: (visible: boolean) =>
                latest().setMixerVisible(visible),
            toggleVuMute: (key: string, codeOnly = false) =>
                latest().mixer.tracks.toggle(key, 'mute', codeOnly),
            toggleVuSolo: (key: string, codeOnly = false) =>
                latest().mixer.tracks.toggle(key, 'solo', codeOnly),
        };
        return () => {
            delete window.__TEST_API__;
        };
    }, []);
}
