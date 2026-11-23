import type { editor } from 'monaco-editor';
import { fontFamilyWithFallback } from '../../../shared/fonts';

/**
 * A line number in a box of its own, which Monaco places in the gutter as
 * markup. Over a picture the box carries the backing, so it is as tall as the
 * line and as wide as the digits.
 */
const renderLineNumber = (lineNumber: number): string =>
    `<span class="line-number-text">${lineNumber}</span>`;

/** The theme settings the editor's construction options follow. */
interface EditorAppearance {
    font: string;
    fontLigatures: boolean;
    fontSize: number;
    cursorStyle: editor.IStandaloneEditorConstructionOptions['cursorStyle'];
}

/**
 * The patch editor's options. The Editor wrapper calls `updateOptions`
 * whenever it receives a new object, so callers keep one object per
 * appearance.
 */
export function patchEditorOptions({
    font,
    fontLigatures,
    fontSize,
    cursorStyle,
}: EditorAppearance): editor.IStandaloneEditorConstructionOptions {
    return {
        minimap: { enabled: false },
        // The transparent editor background lets the scrolled lines bleed
        // through a pinned sticky header as a ghosted duplicate, so the
        // sticky-scroll feature is disabled here.
        stickyScroll: { enabled: false },
        lineNumbers: renderLineNumber,
        folding: false,
        matchBrackets: 'always',
        automaticLayout: true,
        // Monaco's built-in menu hosts a "Command Palette" entry that invokes
        // `editor.action.quickCommand` directly, bypassing the keymap. The
        // native menu of `useEditorMouseGestures` replaces it.
        contextmenu: false,
        fontFamily: fontFamilyWithFallback(font),
        fontLigatures,
        fontSize,
        padding: { bottom: 8, top: 8 },
        renderLineHighlight: 'line',
        cursorBlinking: 'solid',
        cursorStyle,
        scrollbar: {
            horizontal: 'auto',
            horizontalScrollbarSize: 8,
            vertical: 'auto',
            verticalScrollbarSize: 8,
        },
        overviewRulerBorder: false,
        hideCursorInOverviewRuler: true,
        renderLineHighlightOnlyWhenFocus: false,
        guides: {
            bracketPairs: false,
            indentation: true,
        },
        // Overflowing widgets (suggest, hover, parameter hints) render with
        // `position: fixed`, escaping the `.app-main { overflow: hidden }`
        // ancestor. They stay inside the editor's DOM node, so the
        // widget-visibility observer of contextKeyBootstrap still sees them.
        fixedOverflowWidgets: true,
    };
}
