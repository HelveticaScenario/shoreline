import { useEffect, useRef } from 'react';
import type { editor } from 'monaco-editor';
import type { ModelRanges } from '../../app/modelAnchors';
import type { Monaco } from '../../hooks/useCustomMonaco';
import type { ScopeView, VideoPreviewZone } from '../../types/editor';
import {
    type ScopeViewZoneHandle,
    createScopeViewZones,
} from '../monaco/scopeViewZones';
import {
    type VideoPreviewViewZoneHandle,
    createVideoPreviewViewZones,
} from '../monaco/videoPreviewViewZones';

interface EditorViewZonesParams {
    editor: editor.IStandaloneCodeEditor | null;
    monaco: Monaco | null;
    /** The buffer the editor shows; zones of other buffers are left out. */
    currentFile: string | undefined;
    scopeViews: ScopeView[];
    scopeDecorations: ModelRanges | null;
    onRegisterScopeCanvas?: (key: string, canvas: HTMLCanvasElement) => void;
    onUnregisterScopeCanvas?: (key: string) => void;
    videoPreviewZones: VideoPreviewZone[];
    videoPreviewDecorations: ModelRanges | null;
}

/**
 * Shows the active buffer's scopes and `$v.preview` panels as view zones
 * under their calls, rebuilding them when their lists change and moving them
 * as edits shift the calls to other lines.
 */
export function useEditorViewZones({
    editor,
    monaco,
    currentFile,
    scopeViews,
    scopeDecorations,
    onRegisterScopeCanvas,
    onUnregisterScopeCanvas,
    videoPreviewZones,
    videoPreviewDecorations,
}: EditorViewZonesParams) {
    const scopeZonesRef = useRef<ScopeViewZoneHandle | null>(null);
    const previewZonesRef = useRef<VideoPreviewViewZoneHandle | null>(null);

    useEffect(() => {
        if (!editor || !monaco) {
            return;
        }
        const handle = createScopeViewZones({
            editor,
            monaco,
            onRegisterScopeCanvas,
            onUnregisterScopeCanvas,
            scopeDecorations,
            views: scopeViews.filter((view) => view.file === currentFile),
        });
        scopeZonesRef.current = handle;
        return () => {
            handle.dispose();
            scopeZonesRef.current = null;
        };
    }, [
        editor,
        monaco,
        scopeViews,
        currentFile,
        scopeDecorations,
        onRegisterScopeCanvas,
        onUnregisterScopeCanvas,
    ]);

    useEffect(() => {
        if (!editor) {
            return;
        }
        const handle = createVideoPreviewViewZones({
            decorations: videoPreviewDecorations,
            editor,
            zones: videoPreviewZones.filter(
                (zone) => zone.file === currentFile,
            ),
        });
        previewZonesRef.current = handle;
        return () => {
            handle.dispose();
            previewZonesRef.current = null;
        };
    }, [editor, videoPreviewZones, currentFile, videoPreviewDecorations]);

    // The tracked decorations follow each call through edits; the zones
    // re-read them on every content change.
    useEffect(() => {
        if (!editor) {
            return;
        }
        const disposable = editor.onDidChangeModelContent(() => {
            scopeZonesRef.current?.repositionZones();
            previewZonesRef.current?.repositionZones();
        });
        return () => disposable.dispose();
    }, [editor]);
}
