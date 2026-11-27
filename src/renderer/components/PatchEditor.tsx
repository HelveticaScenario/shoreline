import { useEffect, useState } from 'react';
import type { LanguageServerInfo } from '../../shared/ipcTypes';
import electronAPI from '../electronAPI';
import { CodeMirrorPatchEditor } from './codemirror/CodeMirrorPatchEditor';
import { MonacoPatchEditor, type PatchEditorProps } from './MonacoPatchEditor';

/**
 * The patch editor: CodeMirror over the TypeScript language server when the
 * app is launched with MODULAR_EDITOR=codemirror, Monaco otherwise.
 */
export function PatchEditor(props: PatchEditorProps) {
    const [server, setServer] = useState<LanguageServerInfo | null>();
    useEffect(() => {
        electronAPI.languageServer
            .info()
            .then(setServer, () => setServer(null));
    }, []);
    if (server === undefined) {
        return null;
    }
    return server ? (
        <CodeMirrorPatchEditor {...props} languageServer={server} />
    ) : (
        <MonacoPatchEditor {...props} />
    );
}
