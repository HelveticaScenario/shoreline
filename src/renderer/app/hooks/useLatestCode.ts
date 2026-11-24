import { useLayoutEffect, useRef } from 'react';

/**
 * The active buffer's text as the editor has it. The editor's change
 * handler writes it as the text changes, and it follows buffer switches at
 * commit, so a patch submitted before React re-renders never sends stale
 * code.
 */
export function useLatestCode(code: string, onChange: (value: string) => void) {
    const codeRef = useRef(code);
    useLayoutEffect(() => {
        codeRef.current = code;
    }, [code]);

    return {
        getCode: () => codeRef.current,
        handleEditorChange: (value: string) => {
            codeRef.current = value;
            onChange(value);
        },
    };
}
