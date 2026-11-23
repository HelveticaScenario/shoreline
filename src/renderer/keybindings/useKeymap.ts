import { useEffect, useRef } from 'react';
import { evaluateWhen } from './contextKey';
import { loadAndInstallKeymap, setWhenEvaluator } from './keymap';

/**
 * Installs the window-level keymap — the defaults with the user's
 * keybindings.json merged on top — and installs it again whenever
 * `fileRevision`, the count of that file's saves, changes. The installed
 * keymap stays in place until the next one has loaded. Call it after the
 * hooks that register commands: effects run in declaration order, so every
 * binding then finds its handler.
 */
export function useKeymap(fileRevision: number): void {
    const disposeRef = useRef<(() => void) | null>(null);

    useEffect(() => {
        let superseded = false;
        // Gate dispatch on the context-key service so VS Code-style `when`
        // clauses (editorTextFocus, etc.) are honored.
        setWhenEvaluator(evaluateWhen);
        loadAndInstallKeymap()
            .then((result) => {
                if (superseded) {
                    result.dispose();
                    return;
                }
                disposeRef.current?.();
                disposeRef.current = result.dispose;
            })
            .catch((err) => {
                console.error('[keymap] failed to install keymap:', err);
            });
        return () => {
            superseded = true;
        };
    }, [fileRevision]);

    useEffect(
        () => () => {
            disposeRef.current?.();
            disposeRef.current = null;
        },
        [],
    );
}
