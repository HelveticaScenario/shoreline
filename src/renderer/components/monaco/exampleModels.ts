import type { editor } from 'monaco-editor';
import type { Monaco } from '../../hooks/useCustomMonaco';
import { DOCS_EXAMPLE_ROOT } from './docsExampleFiles';

interface Shared {
    model: editor.ITextModel;
    users: number;
}

/**
 * One model per example text in use, shared by every editor showing it. A
 * model lives only while an editor shows it, so the worker's program holds
 * the examples on screen and no others.
 */
const shared = new Map<string, Shared>();
let nextId = 0;

/** The model of `code`, held until `releaseExampleModel`. */
export function acquireExampleModel(
    monaco: Monaco,
    code: string,
): editor.ITextModel {
    let entry = shared.get(code);
    if (!entry) {
        const uri = monaco.Uri.parse(`${DOCS_EXAMPLE_ROOT}${nextId++}.js`);
        entry = {
            model: monaco.editor.createModel(code, 'javascript', uri),
            users: 0,
        };
        shared.set(code, entry);
    }
    entry.users++;
    return entry.model;
}

export function releaseExampleModel(code: string): void {
    const entry = shared.get(code);
    if (entry && --entry.users === 0) {
        shared.delete(code);
        entry.model.dispose();
    }
}
