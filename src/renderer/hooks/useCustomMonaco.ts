import { loader } from '@monaco-editor/react/dist';
import type * as css from '@codingame/monaco-vscode-standalone-css-language-features';
import type * as json from '@codingame/monaco-vscode-standalone-json-language-features';
import type * as typescript from '@codingame/monaco-vscode-standalone-typescript-language-features';
import type * as monaco_editor from 'monaco-editor';
import { useEffect, useState } from 'react';

/** The editor API with the language namespaces of their own packages. */
export type Monaco = typeof monaco_editor & {
    css: typeof css;
    json: typeof json;
    typescript: typeof typescript;
};

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
