import { loader } from '@monaco-editor/react/dist';
import type * as monaco_editor from 'monaco-editor';
import { useEffect, useState } from 'react';

export type Monaco = typeof monaco_editor;

/** The shared Monaco instance, or null until its loader has resolved. */
export function useCustomMonaco(): Monaco | null {
    const [monaco, setMonaco] = useState<Monaco | null>(
        () => loader.__getMonacoInstance() as Monaco | null,
    );

    useEffect(() => {
        if (monaco) {
            return;
        }
        const cancelable = loader.init();
        cancelable
            .then((monacoInstance) => {
                setMonaco(monacoInstance as Monaco);
            })
            .catch((err) => {
                if (err.type !== 'cancelation') {
                    console.error('Monaco initialization error:', err);
                    console.error(
                        'If you are running in dev with StrictMode, this can be safely ignored.',
                    );
                }
            });
        return () => cancelable.cancel();
    }, [monaco]);

    return monaco;
}
