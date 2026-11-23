import React, { useEffect, useState } from 'react';
import Editor from '@monaco-editor/react';
import type { editor } from 'monaco-editor';
import { useTheme } from '../themes/ThemeContext';
import { useCustomMonaco } from '../hooks/useCustomMonaco';
import { configSchema } from '../configSchema';
import { formatPath } from './monaco/monacoHelpers';
import type { ScopeView, VideoPreviewZone } from '../types/editor';
import { applyMonacoTheme } from './monaco/theme';
import { registerConfigSchema } from './monaco/jsonSchema';
import { startModuleStatePolling } from './monaco/moduleStateTracking';
import electronAPI from '../electronAPI';
import type { Schemas } from '../../shared/dsl/schemaTypeResolver';
import type { EditorViewStates } from '../app/editorViewStates';
import { patchEditorOptions } from './patchEditor/editorOptions';
import { useBufferModels } from './patchEditor/useBufferModels';
import { useDslLanguageServices } from './patchEditor/useDslLanguageServices';
import { useEditorBehaviours } from './patchEditor/useEditorBehaviours';
import { useEditorContextMenu } from './patchEditor/useEditorContextMenu';
import { useEditorInstance } from './patchEditor/useEditorInstance';
import { useEditorViewZones } from './patchEditor/useEditorViewZones';

export interface PatchEditorProps {
    value: string;
    currentFile?: string;
    /** Every buffer's cursor and scroll, restored when the editor shows it. */
    viewStates: EditorViewStates;
    /** Ids of every open buffer; models of buffers not listed are disposed. */
    openFiles: string[];
    onChange: (value: string) => void;
    editorRef: React.RefObject<editor.IStandaloneCodeEditor | null>;
    /**
     * Notified when the editor instance becomes available (mount) or goes
     * away (unmount, e.g. last buffer closed). Lets parents subscribe via
     * state instead of reading `editorRef.current` during render.
     */
    onEditorChange?: (editor: editor.IStandaloneCodeEditor | null) => void;
    scopeViews?: ScopeView[];
    /** Tracked decoration collection whose ranges correspond 1:1 with scopeViews. */
    scopeDecorations?: editor.IEditorDecorationsCollection | null;
    onRegisterScopeCanvas?: (key: string, canvas: HTMLCanvasElement) => void;
    onUnregisterScopeCanvas?: (key: string) => void;
    runningBufferId?: string | null;
    /** `$v.preview` panels, anchored by `videoPreviewDecorations`. */
    videoPreviewZones?: VideoPreviewZone[];
    videoPreviewDecorations?: editor.IEditorDecorationsCollection | null;
}

const NO_SCOPE_VIEWS: ScopeView[] = [];
const NO_VIDEO_PREVIEW_ZONES: VideoPreviewZone[] = [];

export function MonacoPatchEditor({
    value,
    currentFile,
    viewStates,
    openFiles,
    onChange,
    editorRef,
    onEditorChange,
    scopeViews = NO_SCOPE_VIEWS,
    scopeDecorations = null,
    onRegisterScopeCanvas,
    onUnregisterScopeCanvas,
    runningBufferId,
    videoPreviewZones = NO_VIDEO_PREVIEW_ZONES,
    videoPreviewDecorations = null,
}: PatchEditorProps) {
    const [libSource, setLibSource] = useState<string | null>(null);
    const [schemas, setSchemas] = useState<Schemas>([]);
    useEffect(() => {
        electronAPI.getDslLibSource().then(setLibSource).catch(console.error);
        electronAPI.getSchemas().then(setSchemas).catch(console.error);
    }, []);

    const monaco = useCustomMonaco();
    const { editor, handleMount } = useEditorInstance(
        editorRef,
        onEditorChange,
    );

    // Highlights the steps the running patch is playing, from the argument
    // and source spans its module states carry. They go on the running
    // buffer's model whether or not the editor shows it, so the performance
    // window shows them too.
    useEffect(() => {
        if (!monaco || !runningBufferId) {
            return;
        }
        const uri = monaco.Uri.parse(formatPath(runningBufferId));
        return startModuleStatePolling({
            getModel: () => monaco.editor.getModel(uri),
            getModuleStates: () =>
                window.electronAPI.synthesizer.getModuleStates(),
            monaco,
        });
    }, [monaco, runningBufferId]);

    useEditorViewZones({
        currentFile,
        editor,
        monaco,
        onRegisterScopeCanvas,
        onUnregisterScopeCanvas,
        scopeDecorations,
        scopeViews,
        videoPreviewDecorations,
        videoPreviewZones,
    });
    useEditorContextMenu(editor);
    const handleEditorChange = useBufferModels({
        currentFile,
        editor,
        monaco,
        onChange,
        openFiles,
        value,
        viewStates,
    });

    const {
        theme: appTheme,
        cursorStyle,
        font,
        fontLigatures,
        fontSize,
        prettierConfig,
    } = useTheme();

    useDslLanguageServices({ libSource, monaco, prettierConfig, schemas });
    useEditorBehaviours({
        dslReady: monaco !== null && schemas.length > 0,
        editor,
        prettierConfig,
    });

    const monacoThemeId = `theme-${appTheme.id}`;
    useEffect(() => {
        if (!monaco) {
            return;
        }
        applyMonacoTheme(monaco, appTheme, monacoThemeId);
    }, [monaco, appTheme, monacoThemeId]);

    // JSON schema for config files.
    useEffect(() => {
        if (!monaco) {
            return;
        }
        registerConfigSchema(monaco, configSchema);
    }, [monaco]);

    // Its own statement, so the object changes only with the appearance:
    // the Editor wrapper calls `updateOptions` for each new one.
    const options = patchEditorOptions({
        cursorStyle,
        font,
        fontLigatures,
        fontSize,
    });

    return (
        <div className="patch-editor" style={{ height: '100%' }}>
            {currentFile && (
                <Editor
                    height="100%"
                    path={formatPath(currentFile)}
                    language={
                        currentFile.endsWith('.json') ? 'json' : 'javascript'
                    }
                    theme={monacoThemeId}
                    defaultValue={value}
                    onChange={handleEditorChange}
                    onMount={handleMount}
                    keepCurrentModel
                    saveViewState={false}
                    options={options}
                />
            )}
        </div>
    );
}
