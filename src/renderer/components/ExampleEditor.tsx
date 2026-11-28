import React, { useEffect, useRef, type RefObject } from 'react';
import type { editor } from 'monaco-editor';
import type { Monaco } from '../hooks/useCustomMonaco';
import { placeContextMenuCaret } from '../keybindings/editorMenuItems';
import { useTheme } from '../themes/ThemeContext';
import {
    acquireExampleModel,
    releaseExampleModel,
} from './monaco/exampleModels';
import {
    forgetExampleEditor,
    showExampleContextMenu,
} from './exampleContextMenu';

interface ExampleEditorProps {
    monaco: Monaco;
    code: string;
    /**
     * The example's box, whose static code the editor covers and takes its
     * place and type from.
     */
    exampleRef: RefObject<HTMLDivElement | null>;
    /** Called once the editor has drawn its first lines, and on dispose. */
    onLive: (live: boolean) => void;
}

/**
 * Where every example draws its hovers and other floating widgets: on the
 * page's body, since a reference card is placed by a transform, which would
 * make the card their frame, clip them to it and let the next card paint
 * over them.
 */
let overflowHost: HTMLElement | null = null;

function overflowWidgetsHost(): HTMLElement {
    if (!overflowHost) {
        // Monaco's widget styles apply within a `.monaco-editor`.
        overflowHost = document.createElement('div');
        overflowHost.className = 'monaco-editor code-example-overflow';
        document.body.append(overflowHost);
    }
    return overflowHost;
}

/** Options matching `pre`'s type and padding, so the code sits where it was. */
function optionsFor(
    pre: HTMLPreElement,
    fontLigatures: boolean,
): editor.IStandaloneEditorConstructionOptions {
    const style = getComputedStyle(pre);
    const px = (value: string) => parseFloat(value) || 0;
    return {
        automaticLayout: false,
        contextmenu: false,
        cursorBlinking: 'solid',
        domReadOnly: true,
        fixedOverflowWidgets: true,
        folding: false,
        fontFamily: style.fontFamily,
        fontLigatures,
        fontSize: px(style.fontSize),
        glyphMargin: false,
        guides: { indentation: false },
        hideCursorInOverviewRuler: true,
        // Monaco has no left padding; the decorations gutter stands in.
        lineDecorationsWidth: px(style.paddingLeft),
        lineHeight: px(style.lineHeight),
        lineNumbers: 'off',
        lineNumbersMinChars: 0,
        matchBrackets: 'never',
        minimap: { enabled: false },
        overflowWidgetsDomNode: overflowWidgetsHost(),
        overviewRulerBorder: false,
        overviewRulerLanes: 0,
        padding: {
            bottom: px(style.paddingBottom),
            top: px(style.paddingTop),
        },
        quickSuggestions: false,
        readOnly: true,
        renderLineHighlight: 'none',
        // The page scrolls under the example; a wide line scrolls sideways.
        scrollBeyondLastLine: false,
        scrollbar: {
            alwaysConsumeMouseWheel: false,
            horizontal: 'auto',
            horizontalScrollbarSize: 6,
            useShadows: false,
            vertical: 'hidden',
        },
        stickyScroll: { enabled: false },
        wordBasedSuggestions: 'off',
    };
}

/**
 * A docs example as a read-only editor laid over its static code: hover,
 * inlay hints and navigation as the patch editor gives them, and nothing
 * that edits.
 */
export const ExampleEditor: React.FC<ExampleEditorProps> = ({
    monaco,
    code,
    exampleRef,
    onLive,
}) => {
    const hostRef = useRef<HTMLDivElement>(null);
    const { fontLigatures } = useTheme();

    useEffect(() => {
        const host = hostRef.current;
        const pre =
            exampleRef.current?.querySelector<HTMLPreElement>(':scope > pre');
        if (!host || !pre) {
            return;
        }
        const model = acquireExampleModel(monaco, code);
        const ed = monaco.editor.create(host, {
            ...optionsFor(pre, fontLigatures),
            model,
        });

        // The box follows the static code's; a peek opened inside the editor
        // makes it taller, and the static code with it.
        const layout = () => {
            const height = ed.getContentHeight();
            pre.style.minHeight = '';
            if (height > pre.offsetHeight) {
                pre.style.minHeight = `${height}px`;
            }
            host.style.top = `${pre.offsetTop}px`;
            host.style.left = `${pre.offsetLeft}px`;
            const width = pre.offsetWidth;
            host.style.width = `${width}px`;
            host.style.height = `${pre.offsetHeight}px`;
            ed.layout({ height: pre.offsetHeight, width });
        };
        layout();
        const resize = new ResizeObserver(layout);
        resize.observe(pre);
        const contentSize = ed.onDidContentSizeChange((e) => {
            if (e.contentHeightChanged) {
                layout();
            }
        });
        const contextMenu = ed.onContextMenu((e) => {
            placeContextMenuCaret(ed, e.target.position);
            showExampleContextMenu(ed);
        });
        const frame = requestAnimationFrame(() => onLive(true));

        return () => {
            cancelAnimationFrame(frame);
            onLive(false);
            contextMenu.dispose();
            contentSize.dispose();
            resize.disconnect();
            forgetExampleEditor(ed);
            ed.dispose();
            releaseExampleModel(code);
            pre.style.minHeight = '';
        };
    }, [monaco, code, exampleRef, fontLigatures, onLive]);

    return <div ref={hostRef} className="code-example-editor" />;
};
