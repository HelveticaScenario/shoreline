// @vitest-environment jsdom

/**
 * usePerformanceCode owns one PerformanceCode per mount: the instance the
 * performance output draws is never a disposed one, it holds the hook's
 * current arguments, and unmounting disposes every instance created.
 */

import type { editor } from 'monaco-editor';
import { act, createElement, StrictMode, useEffect } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, expect, test, vi } from 'vitest';

const fakes = vi.hoisted(() => {
    class FakeCode {
        disposed = false;
        editor: unknown = undefined;
        bufferPath: string | null | undefined = undefined;
        revealed: number[] = [];
        constructor() {
            shared.created.push(this);
        }
        attach(editor: unknown) {
            this.editor = editor;
        }
        showBuffer(path: string | null) {
            this.bufferPath = path;
        }
        setControls() {}
        setLook() {}
        reveal(offset: number) {
            this.revealed.push(offset);
        }
        dispose() {
            this.disposed = true;
        }
    }
    const shared = {
        FakeCode,
        created: [] as FakeCode[],
        monaco: {},
        output: {
            code: null as FakeCode | null,
            setCode(code: FakeCode | null) {
                shared.output.code = code;
            },
            setCodeOpacity() {},
        },
    };
    return shared;
});

vi.mock('../hooks/useCustomMonaco', () => ({
    useCustomMonaco: () => fakes.monaco,
}));
vi.mock('../video/PerformanceOutput', () => ({
    performanceOutput: fakes.output,
}));
vi.mock('./PerformanceCode', () => ({ PerformanceCode: fakes.FakeCode }));

import type { AppTheme } from '../themes/types';
import type { PerformanceViewSettings } from './settings';
import { usePerformanceCode } from './usePerformanceCode';

const EDITOR = {} as editor.IStandaloneCodeEditor;
const SETTINGS = {
    codeOpacity: 1,
    codeVisible: true,
} as PerformanceViewSettings;
const THEME = {} as AppTheme;

let root: Root | null = null;
const CONTROLS: never[] = [];
const hook = { reveal: (_offset: number): void => {} };

function Probe({ bufferPath }: { bufferPath: string | null }) {
    const reveal = usePerformanceCode(
        EDITOR,
        bufferPath,
        SETTINGS,
        THEME,
        CONTROLS,
    );
    useEffect(() => {
        hook.reveal = reveal;
    });
    return null;
}

function render(bufferPath: string | null) {
    act(() => {
        root!.render(
            createElement(
                StrictMode,
                null,
                createElement(Probe, { bufferPath }),
            ),
        );
    });
}

beforeEach(() => {
    (
        globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }
    ).IS_REACT_ACT_ENVIRONMENT = true;
    fakes.created.length = 0;
    fakes.output.code = null;
    root = createRoot(document.createElement('div'));
});

afterEach(() => {
    act(() => root?.unmount());
    root = null;
});

test('the instance in use under StrictMode is live and holds the arguments', () => {
    render('a.mjs');

    const code = fakes.output.code;
    expect(code).not.toBeNull();
    expect(code!.disposed).toBe(false);
    expect(code!.editor).toBe(EDITOR);
    expect(code!.bufferPath).toBe('a.mjs');
    expect(fakes.created.filter((created) => created !== code)).toSatisfy(
        (others: { disposed: boolean }[]) =>
            others.every((other) => other.disposed),
    );

    render('b.mjs');
    expect(fakes.output.code).toBe(code);
    expect(code!.bufferPath).toBe('b.mjs');

    hook.reveal(7);
    expect(code!.revealed).toEqual([7]);
});

test('unmounting disposes every instance and clears the output', () => {
    render(null);
    act(() => root?.unmount());
    root = null;

    expect(fakes.created.length).toBeGreaterThan(0);
    expect(fakes.created.every((created) => created.disposed)).toBe(true);
    expect(fakes.output.code).toBeNull();
});
