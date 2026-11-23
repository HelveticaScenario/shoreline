import { useEffect } from 'react';
import { evaluateWhen } from './contextKey';
import { loadAndInstallKeymap, setWhenEvaluator } from './keymap';
import { KEYBINDINGS_CHANGED_EVENT } from './useKeybindingsFile';

/**
 * Installs the window-level keymap — the defaults with the user's
 * keybindings.json merged on top — and re-installs it whenever that file is
 * saved. Call it after the hooks that register commands: effects run in
 * declaration order, so every binding then finds its handler.
 */
export function useKeymap(): void {
    useEffect(() => {
        let disposed = false;
        let disposer: (() => void) | null = null;
        // Gate dispatch on the context-key service so VS Code-style `when`
        // clauses (editorTextFocus, etc.) are honored.
        setWhenEvaluator(evaluateWhen);
        const install = () => {
            loadAndInstallKeymap()
                .then((result) => {
                    if (disposed) {
                        result.dispose();
                        return;
                    }
                    disposer?.();
                    disposer = result.dispose;
                })
                .catch((err) => {
                    console.error('[keymap] failed to install keymap:', err);
                });
        };
        install();
        window.addEventListener(KEYBINDINGS_CHANGED_EVENT, install);
        return () => {
            disposed = true;
            window.removeEventListener(KEYBINDINGS_CHANGED_EVENT, install);
            disposer?.();
        };
    }, []);

    // Ctrl+Enter (and Ctrl+Shift+Enter) are reserved for patch updates.
    // Browsers activate a focused <button> on Enter regardless of modifier
    // state, which would toggle e.g. the Link button after it had been
    // clicked, so the default activation is suppressed.
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
