import { useState } from 'react';
import type { editor } from 'monaco-editor';
import type { ButtonDefinition } from '../../../shared/dsl/buttonTypes';
import { formatPath } from '../../components/monaco/monacoHelpers';
import { usePerformanceCode } from '../../performance/usePerformanceCode';
import { useTheme } from '../../themes/useTheme';
import type { EditorBuffer } from '../../types/editor';
import { performanceOutput } from '../../video/PerformanceOutput';
import { getBufferId } from '../buffers';
import type { bindControls } from '../controlBinding';
import { performanceControls } from '../performanceControls';

type ControlViews = ReturnType<typeof bindControls>;

/**
 * Lays the playing buffer's code and controls over the performance window.
 * The window shows the playing buffer from when its patch starts playing
 * until another buffer's does, stop included. It finds the buffer by id,
 * which survives renames. Once no tab holds that id, it shows the buffer's
 * file while another tab has it open, and otherwise the visible buffer; a
 * closed untitled buffer is gone for good, since a new one reuses its id.
 * Before any patch has played it shows the visible buffer.
 *
 * Returns a function that scrolls the window to a control of the visible
 * buffer, so the audience sees which control is being played; it does
 * nothing while the window shows another buffer.
 */
export function usePerformanceWindow({
    buffers,
    activeBufferId,
    runningSourceId,
    visibleControls,
    bindBuffer,
    buttonDefs,
    editorInstance,
}: {
    buffers: EditorBuffer[];
    activeBufferId: string | undefined;
    /** Stable id of the buffer the running patch was evaluated from. */
    runningSourceId: string | null;
    visibleControls: ControlViews;
    bindBuffer: (bufferId: string) => ControlViews;
    buttonDefs: ButtonDefinition[];
    editorInstance: editor.IStandaloneCodeEditor | null;
}): (offset: number) => void {
    const { performanceView, theme, themes } = useTheme();
    const [source, setSource] = useState<{
        id: string | null;
        closed: boolean;
        /** The source buffer's file, as last seen in a tab. */
        lastFile: string | null;
    }>({ closed: false, id: null, lastFile: null });

    let current = source;
    if (runningSourceId !== null && runningSourceId !== current.id) {
        current = { closed: false, id: runningSourceId, lastFile: null };
    }
    const buffer =
        current.id === null || current.closed
            ? undefined
            : buffers.find((b) => b.id === current.id);
    if (current.id !== null && !current.closed && buffer === undefined) {
        current = { ...current, closed: true };
    }
    const file = buffer?.kind === 'file' ? formatPath(buffer.filePath) : null;
    if (file !== null && file !== current.lastFile) {
        current = { ...current, lastFile: file };
    }
    if (current !== source) {
        setSource(current);
    }

    const { lastFile } = current;
    const bufferPath =
        buffer === undefined
            ? lastFile !== null &&
              buffers.some((b) => formatPath(getBufferId(b)) === lastFile)
                ? lastFile
                : null
            : formatPath(getBufferId(buffer));
    const showsVisible =
        bufferPath === null ||
        (!!activeBufferId && bufferPath === formatPath(activeBufferId));

    const views =
        showsVisible || current.id === null
            ? visibleControls
            : bindBuffer(current.id);
    const reveal = usePerformanceCode(
        editorInstance,
        bufferPath,
        performanceView,
        themes.find((t) => t.id === performanceView.themeId) ?? theme,
        performanceControls(views.sliders, views.buttons, buttonDefs),
    );

    return (offset: number) => {
        if (performanceOutput.isOpen && showsVisible) {
            reveal(offset);
        }
    };
}
