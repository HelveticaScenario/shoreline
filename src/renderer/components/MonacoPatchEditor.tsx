import { useEffect, useState } from 'react';
import Editor from '@monaco-editor/react';
import type { editor } from 'monaco-editor';
import { useTheme } from '../themes/ThemeContext';
import { useCustomMonaco } from '../hooks/useCustomMonaco';
import { formatPath } from './monaco/monacoHelpers';
import type { ScopeView, VideoPreviewZone } from '../types/editor';
import electronAPI from '../electronAPI';
import type { Schemas } from '../../shared/dsl/schemaTypeResolver';
import type { EditorViewStates } from '../app/editorViewStates';
import type { ModelRanges } from '../app/modelAnchors';
import { editorCodeFormat } from './patchEditor/codeFormat';
import { patchEditorOptions } from './patchEditor/editorOptions';
import { useBufferModels } from './patchEditor/useBufferModels';
import { useCodeDecorations } from './patchEditor/useCodeDecorations';
import { useEditorContextKeys } from './patchEditor/useEditorContextKeys';
import { useEditorInstance } from './patchEditor/useEditorInstance';
import { useEditorMouseGestures } from './patchEditor/useEditorMouseGestures';
import { useEditorViewZones } from './patchEditor/useEditorViewZones';
import { useMonacoRegistrations } from './patchEditor/useMonacoRegistrations';

export interface PatchEditorProps {
    value: string;
    currentFile?: string;
    /** Every buffer's cursor and scroll, restored when the editor shows it. */
    viewStates: EditorViewStates;
    /** Ids of every open buffer; models of buffers not listed are disposed. */
    openFiles: string[];
    onChange: (value: string) => void;
    /** Called with the editor as it mounts and with null as it goes away (the
     *  last buffer closed), from inside Monaco's own callbacks, so what it
     *  stores is readable before the next render. The editor keeps the
     *  function it mounted with, so it must not read values that change
     *  between renders. */
    onEditorChange: (editor: editor.IStandaloneCodeEditor | null) => void;
    scopeViews?: ScopeView[];
    /** Tracked decoration collection whose ranges correspond 1:1 with scopeViews. */
    scopeDecorations?: ModelRanges | null;
    onRegisterScopeCanvas?: (key: string, canvas: HTMLCanvasElement) => void;
    onUnregisterScopeCanvas?: (key: string) => void;
    runningBufferId?: string | null;
    /** `$v.preview` panels, anchored by `videoPreviewDecorations`. */
    videoPreviewZones?: VideoPreviewZone[];
    videoPreviewDecorations?: ModelRanges | null;
}

const NO_SCOPE_VIEWS: ScopeView[] = [];
const NO_VIDEO_PREVIEW_ZONES: VideoPreviewZone[] = [];

export function MonacoPatchEditor({
    value,
    currentFile,
    viewStates,
    openFiles,
    onChange,
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
    const { editor, handleMount } = useEditorInstance(onEditorChange);

    const {
        theme: appTheme,
        cursorStyle,
        font,
        fontLigatures,
        fontSize,
        prettierConfig,
    } = useTheme();
    const codeFormat = editorCodeFormat(prettierConfig);
    const monacoThemeId = `theme-${appTheme.id}`;

    useMonacoRegistrations({
        codeFormat,
        libSource,
        monaco,
        schemas,
        theme: appTheme,
        themeId: monacoThemeId,
    });
    // Before useBufferModels: a buffer's view zones are in place when its
    // view state is restored, so the restored scroll position counts them.
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
    const handleEditorChange = useBufferModels({
        currentFile,
        editor,
        monaco,
        onChange,
        openFiles,
        tabSize: codeFormat.tabSize,
        value,
        viewStates,
    });
    useCodeDecorations({ editor, monaco, runningBufferId });
    useEditorContextKeys(editor);
    useEditorMouseGestures({
        codeFormat,
        dslReady: monaco !== null && schemas.length > 0,
        editor,
    });

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
