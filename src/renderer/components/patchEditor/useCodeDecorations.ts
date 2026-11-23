import { useEffect } from 'react';
import type { editor } from 'monaco-editor';
import type { Monaco } from '../../hooks/useCustomMonaco';
import { trackCodeBacking } from '../monaco/codeBacking';
import { formatPath } from '../monaco/monacoHelpers';
import { startModuleStatePolling } from '../monaco/moduleStateTracking';

interface CodeDecorationsParams {
    editor: editor.IStandaloneCodeEditor | null;
    monaco: Monaco | null;
    runningBufferId: string | null | undefined;
}

/**
 * Keeps the decorations drawn on the code itself: the highlights of the
 * steps the running patch is playing, and the backing behind each line's
 * text.
 */
export function useCodeDecorations({
    editor,
    monaco,
    runningBufferId,
}: CodeDecorationsParams) {
    // The highlights come from the argument and source spans the module
    // states carry. They go on the running buffer's model whether or not the
    // editor shows it, so the performance window shows them too.
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

    useEffect(() => {
        if (!editor) {
            return;
        }
        return trackCodeBacking(editor);
    }, [editor]);
}
