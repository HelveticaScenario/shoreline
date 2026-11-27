import { useEffect, useEffectEvent, useState } from 'react';
import type { editor } from 'monaco-editor';
import './App.css';
import { formatBufferLabel, getBufferId } from './app/buffers';
import { useAppCommands } from './app/hooks/useAppCommands';
import { useControlActions, useControlViews } from './app/hooks/useControls';
import { useEditorBuffers } from './app/hooks/useEditorBuffers';
import { type EngineFrame, useEngineFrames } from './app/hooks/useEngineFrames';
import { useFileActions } from './app/hooks/useFileActions';
import { useLatestCode } from './app/hooks/useLatestCode';
import { usePatchErrors } from './app/hooks/usePatchErrors';
import { usePatchRunner } from './app/hooks/usePatchRunner';
import { usePerformanceWindow } from './app/hooks/usePerformanceWindow';
import { useRecording } from './app/hooks/useRecording';
import { useTestApi } from './app/hooks/useTestApi';
import { useWorkspace } from './app/hooks/useWorkspace';
import { ScopeCanvases } from './app/scopeCanvases';
import { updateTransport } from './app/transportStore';
import { PerformancePreview } from './app/video/PerformancePreview';
import { VideoBackdrop } from './app/video/VideoBackdrop';
import { AppUpdates } from './components/AppUpdates';
import { AudioControls } from './components/AudioControls';
import { AudioDeviceBanner } from './components/AudioDeviceBanner';
import { AudioPanicDialog } from './components/AudioPanicDialog';
import { BottomBar } from './components/BottomBar';
import { BottomPanel } from './components/BottomPanel';
import { CommandPalette } from './components/CommandPalette';
import { ControlPanel } from './components/ControlPanel';
import { EngineHealth } from './components/EngineHealth';
import { ErrorDisplay } from './components/ErrorDisplay';
import { FileExplorer } from './components/FileExplorer';
import { ModuleProfile } from './components/ModuleProfile';
import { PatchEditor } from './components/PatchEditor';
import { PerformancePanel } from './components/PerformancePanel';
import { SaveConflicts } from './components/SaveConflicts';
import { Sidebar } from './components/Sidebar';
import { TransportDisplay } from './components/TransportDisplay';
import { VuMeterPanel } from './components/VuMeterPanel';
import { resolvePrettierOptions } from './components/monaco/formattingProvider';
import { codeStyleFromPrettier } from './dsl/listEdits';
import electronAPI from './electronAPI';
import { EqPanelController } from './eq/eqPanelController';
import { useBottomPanel } from './hooks/useBottomPanel';
import { useEditableConfig } from './hooks/useEditableConfig';
import { useKeybindingsFile } from './keybindings/useKeybindingsFile';
import { useKeymap } from './keybindings/useKeymap';
import { useReservedCtrlEnter } from './keybindings/useReservedCtrlEnter';
import { MixerController } from './mixer/MixerController';
import { useMixerJump } from './mixer/useMixerJump';
import { useTheme } from './themes/ThemeContext';
import type { EditorBuffer } from './types/editor';

const createEqController = () =>
    new EqPanelController({
        getModuleStates: () => electronAPI.synthesizer.getModuleStates(),
        getSampleRate: () => electronAPI.synthesizer.getSampleRate(),
        setModuleParam: (moduleId, moduleType, params) =>
            electronAPI.synthesizer.setModuleParam(
                moduleId,
                moduleType,
                params as object,
            ),
    });

const createMixer = () =>
    new MixerController({
        setModuleParam: (moduleId, moduleType, params) =>
            electronAPI.synthesizer.setModuleParam(
                moduleId,
                moduleType,
                params,
            ),
    });

const toggleLink = (enabled: boolean) => {
    void electronAPI.synthesizer.enableLink(enabled);
    // The transport is only polled while playing, so show the change now.
    updateTransport((prev) => ({
        ...prev,
        linkEnabled: enabled,
        linkPeers: enabled ? prev.linkPeers : 0,
    }));
};

function App() {
    const { codeBackdropOpacity, performanceScale, prettierConfig } =
        useTheme();
    const prettierOptions = resolvePrettierOptions(prettierConfig);
    const codeStyle = codeStyleFromPrettier(prettierOptions);

    const [editorInstance, setEditorInstance] =
        useState<editor.IStandaloneCodeEditor | null>(null);
    /** Inline `ui: true` EQ panels of the running patch. */
    const [eqController] = useState(createEqController);
    const [mixer] = useState(createMixer);
    const [scopeCanvases] = useState(() => new ScopeCanvases());
    const [isEngineHealthOpen, setIsEngineHealthOpen] = useState(false);
    const [isModuleProfileOpen, setIsModuleProfileOpen] = useState(false);
    const [isPaletteOpen, setIsPaletteOpen] = useState(false);

    const [appConfig, changeAppConfig, appConfigLoaded] =
        useEditableConfig(true);
    const bottomPanel = useBottomPanel(
        appConfig,
        changeAppConfig,
        appConfigLoaded,
    );
    const isMixerVisible = bottomPanel.tab === 'mixer';

    const errors = usePatchErrors();
    const workspace = useWorkspace();
    const keybindingsFile = useKeybindingsFile();
    const files = useEditorBuffers({
        onFileSaved: keybindingsFile.onFileSaved,
        quotes: prettierOptions.quotes,
        refreshFileTree: workspace.refreshFileTree,
        workspaceRoot: workspace.root,
    });
    const { buffers, activeBufferId, patchCode } = files;
    // The active tab's id. Unlike `activeBufferId`, a file's mutable path,
    // it survives rename and save.
    const activeSourceId =
        buffers.find((b) => getBufferId(b) === activeBufferId)?.id ??
        activeBufferId;
    const fileActions = useFileActions({
        chooseWorkspace: workspace.choose,
        ensureKeybindingsFile: keybindingsFile.ensureFile,
        files,
        showError: errors.show,
    });

    const { getCode, handleEditorChange } = useLatestCode(
        patchCode,
        files.handlePatchChange,
    );
    const patch = usePatchRunner({
        activeBufferId,
        activeSourceId,
        buffers,
        eqController,
        errors,
        getCode,
        mixer,
    });
    const { isClockRunning, patchUi } = patch;
    const recording = useRecording();

    const controls = useControlViews(activeSourceId, patchCode, patchUi);
    const revealInPerformance = usePerformanceWindow({
        activeBufferId,
        bindBuffer: controls.bindBuffer,
        buffers,
        buttonDefs: patchUi.buttonDefs,
        editorInstance,
        runningSourceId: patchUi.sourceId,
        visibleControls: controls.views,
    });
    const controlActions = useControlActions({
        codeStyle,
        editorInstance,
        onUse: revealInPerformance,
        setButtonValue: patch.setButtonValue,
        setSliderValue: patch.setSliderValue,
        views: controls.views,
    });

    useEffect(() => {
        eqController.attachEditor(editorInstance);
        mixer.edits.attachEditor(editorInstance);
    }, [eqController, mixer, editorInstance]);
    useEffect(() => {
        eqController.setCodeStyle(codeStyle);
    }, [eqController, codeStyle]);
    useEffect(() => {
        if (!isClockRunning) {
            eqController.onStopped();
            mixer.meters.silence();
        }
    }, [eqController, mixer, isClockRunning]);
    useEffect(() => () => eqController.dispose(), [eqController]);

    const jumpToTrack = useMixerJump({
        buffers,
        jumpTo: controlActions.jumpTo,
        mixer,
        showBuffer: files.setActiveBufferId,
        visibleBufferId: activeSourceId,
    });

    useEngineFrames({
        onFrame: ({ scopes, meters, transport }: EngineFrame) => {
            eqController.onScopeFrames(scopes, performance.now());
            scopeCanvases.draw(scopes);
            mixer.meters.drawFrames(meters);
            patch.settleQueuedUpdate(transport);
        },
        pollMeters: isMixerVisible,
        running: isClockRunning,
    });

    // An output device change that stopped playback leaves the editor in
    // the same state as Stop.
    const onDeviceNotice = useEffectEvent(
        (notice: { stopped: boolean; recordingStopped: boolean }) => {
            if (notice.stopped) {
                void patch.stop();
            }
            if (notice.recordingStopped) {
                recording.markStopped();
            }
        },
    );
    useEffect(
        () => electronAPI.audio.onDeviceNotice((n) => onDeviceNotice(n)),
        [],
    );

    useAppCommands({
        closeActiveBuffer: () => {
            if (activeBufferId) {
                void files.closeBuffer(activeBufferId);
            }
        },
        newFile: files.createUntitledFile,
        openEngineHealth: () => setIsEngineHealthOpen(true),
        openKeybindings: () => void fileActions.openKeybindings(),
        openModuleProfile: () => setIsModuleProfileOpen(true),
        openWorkspace: () => void fileActions.switchWorkspace(),
        save: () => void fileActions.save(),
        showCommandPalette: () => setIsPaletteOpen(true),
        stop: () => void patch.stop(),
        submit: (trigger) => void patch.submit(trigger),
        toggleRecording: recording.toggle,
    });
    useKeymap(keybindingsFile.revision);
    useReservedCtrlEnter();
    useTestApi({
        eqController,
        getLastResult: patch.getLastResult,
        isClockRunning,
        mixer,
        openEngineHealth: () => setIsEngineHealthOpen(true),
        openModuleProfile: () => setIsModuleProfileOpen(true),
        setMixerVisible: (visible) =>
            bottomPanel.show(visible ? 'mixer' : null),
        submit: () => void patch.submit(),
    });

    const fileName = (buffer: EditorBuffer) =>
        formatBufferLabel(buffer).split(/[/\\]/).at(-1)!;

    return (
        <div className="app">
            <header className="app-header">
                <TransportDisplay onToggleLink={toggleLink} />
                <AudioControls
                    isRunning={isClockRunning}
                    isRecording={recording.isRecording}
                    onStop={patch.stop}
                    onStartRecording={recording.start}
                    onStopRecording={recording.stop}
                    onUpdatePatch={() => patch.submit()}
                />
            </header>

            <ErrorDisplay
                error={errors.error}
                errors={errors.validationErrors}
                onDismiss={errors.clear}
            />
            <EngineHealth
                isOpen={isEngineHealthOpen}
                onClose={() => setIsEngineHealthOpen(false)}
            />
            <ModuleProfile
                isOpen={isModuleProfileOpen}
                onClose={() => setIsModuleProfileOpen(false)}
            />
            <AudioPanicDialog />
            <AudioDeviceBanner playing={isClockRunning} />
            <CommandPalette
                open={isPaletteOpen}
                onOpenChange={setIsPaletteOpen}
                editor={editorInstance}
            />

            <main className="app-main">
                <div className="app-workspace">
                    {!workspace.root ? (
                        <div className="empty-state">
                            <button
                                className="open-folder-button"
                                onClick={fileActions.switchWorkspace}
                            >
                                Open Folder
                            </button>
                        </div>
                    ) : (
                        <div className="editor-panel">
                            <VideoBackdrop
                                codeBackdropOpacity={codeBackdropOpacity}
                                performanceScale={performanceScale}
                            />
                            <PatchEditor
                                value={patchCode}
                                runningBufferId={patch.runningBufferId}
                                currentFile={activeBufferId}
                                viewStates={files.viewStates}
                                openFiles={buffers.map(getBufferId)}
                                onChange={handleEditorChange}
                                onEditorChange={setEditorInstance}
                                scopeViews={patchUi.scopeViews}
                                scopeDecorations={patchUi.scopeDecorations}
                                videoPreviewDecorations={
                                    patchUi.videoPreviews.decorations
                                }
                                videoPreviewZones={patchUi.videoPreviews.zones}
                                onRegisterScopeCanvas={scopeCanvases.register}
                                onUnregisterScopeCanvas={
                                    scopeCanvases.unregister
                                }
                            />
                            <PerformancePreview />
                        </div>
                    )}
                    <BottomPanel
                        state={bottomPanel}
                        mixer={
                            <VuMeterPanel mixer={mixer} onJump={jumpToTrack} />
                        }
                    />
                </div>
                <Sidebar
                    docs={bottomPanel.docs}
                    config={appConfig}
                    onConfigChange={changeAppConfig}
                    configLoaded={appConfigLoaded}
                    explorerContent={
                        <FileExplorer
                            workspaceRoot={workspace.root}
                            fileTree={workspace.fileTree}
                            buffers={buffers}
                            activeBufferId={activeBufferId}
                            runningBufferId={patch.runningBufferId}
                            renamingPath={files.renamingPath}
                            formatLabel={fileName}
                            onSelectBuffer={files.setActiveBufferId}
                            onOpenFile={fileActions.open}
                            onCreateFile={files.createUntitledFile}
                            onCloseBuffer={files.closeBuffer}
                            onRefreshTree={workspace.refreshFileTree}
                            onRenameCommit={fileActions.commitRename}
                            onRenameCancel={() => files.setRenamingPath(null)}
                            onKeepBuffer={files.keepBuffer}
                        />
                    }
                    performContent={<PerformancePanel />}
                    controlContent={
                        <ControlPanel
                            sliders={controls.views.sliders}
                            buttons={controls.views.buttons}
                            groups={controls.groups}
                            onGroupToggle={controlActions.toggleGroup}
                            onSliderChange={controlActions.changeSlider}
                            onButtonChange={controlActions.changeButton}
                            onJump={controlActions.jumpTo}
                        />
                    }
                />
            </main>
            <BottomBar
                tab={bottomPanel.tab}
                onToggle={bottomPanel.toggle}
                docsLocation={bottomPanel.docs.location}
            />
            <SaveConflicts files={files} showError={errors.show} />
            <AppUpdates />
        </div>
    );
}

export default App;
