import { useEffect } from 'react';

/**
 * Keeps Ctrl+Enter (and Ctrl+Shift+Enter), reserved for patch updates, from
 * activating a focused button. Browsers activate a focused `<button>` on
 * Enter regardless of modifier state, which would toggle e.g. the Link
 * button after it had been clicked.
 */
export function useReservedCtrlEnter(): void {
    useEffect(() => {
        const onKeyDownCapture = (e: KeyboardEvent) => {
            if (
                e.ctrlKey &&
                e.key === 'Enter' &&
                e.target instanceof HTMLButtonElement
            ) {
                e.preventDefault();
                e.stopPropagation();
            }
        };
        window.addEventListener('keydown', onKeyDownCapture, { capture: true });
        return () =>
            window.removeEventListener('keydown', onKeyDownCapture, {
                capture: true,
            });
    }, []);
}
