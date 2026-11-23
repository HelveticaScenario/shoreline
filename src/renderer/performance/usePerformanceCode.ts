import type { editor } from 'monaco-editor';
import { useEffect, useRef } from 'react';
import { useCustomMonaco } from '../hooks/useCustomMonaco';
import type { AppTheme } from '../themes/types';
import { performanceOutput } from '../video/PerformanceOutput';
import { PerformanceCode, type PlacedControl } from './PerformanceCode';
import type { PerformanceViewSettings } from './settings';

/**
 * Lays the code of the buffer whose model is at `bufferPath` (null for the
 * buffer `codeEditor` shows) over the performance window's picture, drawn
 * with `settings` in the colors of `theme`, with a widget for each of
 * `controls`. Returns a function that scrolls the performance view to an
 * offset in that buffer's text (see {@link PerformanceCode.reveal}).
 */
export function usePerformanceCode(
    codeEditor: editor.IStandaloneCodeEditor | null,
    bufferPath: string | null,
    settings: PerformanceViewSettings,
    theme: AppTheme,
    controls: readonly PlacedControl[],
): (offset: number) => void {
    const monaco = useCustomMonaco();
    const codeRef = useRef<PerformanceCode | null>(null);

    // Each run owns the instance it creates. The effects below run after
    // this one and name `monaco` as the instance's identity, so a new
    // instance is given everything again.
    useEffect(() => {
        if (monaco === null) return;
        const code = new PerformanceCode(monaco);
        codeRef.current = code;
        performanceOutput.setCode(code);
        return () => {
            codeRef.current = null;
            performanceOutput.setCode(null);
            code.dispose();
        };
    }, [monaco]);

    useEffect(() => {
        codeRef.current?.attach(codeEditor);
    }, [monaco, codeEditor]);

    useEffect(() => {
        codeRef.current?.showBuffer(bufferPath);
    }, [monaco, bufferPath]);

    useEffect(() => {
        codeRef.current?.setControls(controls);
    }, [monaco, controls]);

    useEffect(() => {
        codeRef.current?.setLook(settings, theme);
    }, [monaco, settings, theme]);

    useEffect(() => {
        performanceOutput.setCodeOpacity(
            settings.codeVisible ? settings.codeOpacity : 0,
        );
    }, [settings.codeVisible, settings.codeOpacity]);

    return (offset: number) => codeRef.current?.reveal(offset);
}
