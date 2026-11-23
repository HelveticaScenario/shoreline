import { useEffect, useEffectEvent, type RefObject } from 'react';
import type { editor } from 'monaco-editor';
import electronAPI from '../../electronAPI';
import type { EqPanelController } from '../../eq/eqPanelController';
import { executeCommand } from '../../keybindings/commands';
import type { MixerController } from '../../mixer/MixerController';

/** Exposes `window.__TEST_API__`, which the E2E tests drive the app through. */
export function useTestApi(app: {
    /** Read on each call, so a call made as the editor mounts reaches it. */
    editorRef: RefObject<editor.IStandaloneCodeEditor | null>;
    eqController: EqPanelController;
    mixer: MixerController;
    isClockRunning: boolean;
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
                const editorNow = latest().editorRef.current;
                const model = editorNow?.getModel();
                const position = editorNow?.getPosition();
                return model && position ? model.getOffsetAt(position) : null;
            },
            getEditorValue: () => latest().editorRef.current?.getValue() ?? '',
            getEqPanels: () => latest().eqController.snapshot(),
            getLastPatchResult: () => latest().getLastResult(),
            getScopeData: () => electronAPI.synthesizer.getScopes(),
            getVuMeterData: () => electronAPI.synthesizer.getVuMeters(),
            getVuOutputs: () => latest().mixer.store.outputs,
            isClockRunning: () => latest().isClockRunning,
            newUntitledFile: () => executeCommand('shoreline.newFile'),
            openEngineHealth: () => latest().openEngineHealth(),
            openModuleProfile: () => latest().openModuleProfile(),
            setEditorValue: (code: string) =>
                latest().editorRef.current?.setValue(code),
            setMixerVisible: (visible: boolean) =>
                latest().setMixerVisible(visible),
            toggleVuMute: (key: string, codeOnly = false) =>
                latest().mixer.toggle(key, 'mute', codeOnly),
            toggleVuSolo: (key: string, codeOnly = false) =>
                latest().mixer.toggle(key, 'solo', codeOnly),
        };
        return () => {
            delete window.__TEST_API__;
        };
    }, []);
}
