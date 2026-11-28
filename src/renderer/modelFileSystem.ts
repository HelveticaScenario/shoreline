import type { IDisposable } from 'monaco-editor';
import * as monaco from 'monaco-editor';
import {
    FileSystemProviderCapabilities,
    FileSystemProviderError,
    FileSystemProviderErrorCode,
    FileType,
    registerFileSystemOverlay,
} from '@codingame/monaco-vscode-files-service-override';

const encoder = new TextEncoder();
const never = () => ({ dispose() {} });

function modelAt(resource: monaco.Uri): monaco.editor.ITextModel {
    const model = monaco.editor.getModel(resource);
    if (!model) {
        throw FileSystemProviderError.create(
            'no model',
            FileSystemProviderErrorCode.FileNotFound,
        );
    }
    return model;
}

/**
 * Serves the open models as read-only files. The VS Code API resolves a
 * `file:` model through its file service even when the model is open, as
 * go to definition and peek do; buffers live only as models.
 */
export function registerModelFileSystem(): IDisposable {
    return registerFileSystemOverlay(1, {
        capabilities:
            FileSystemProviderCapabilities.FileReadWrite |
            FileSystemProviderCapabilities.PathCaseSensitive |
            FileSystemProviderCapabilities.Readonly,
        onDidChangeCapabilities: never,
        onDidChangeFile: never,
        watch: never,
        async stat(resource: monaco.Uri) {
            const model = modelAt(resource);
            return {
                ctime: 0,
                mtime: model.getVersionId(),
                size: model.getValueLength(),
                type: FileType.File,
            };
        },
        async readFile(resource: monaco.Uri) {
            return encoder.encode(modelAt(resource).getValue());
        },
        async readdir() {
            return [];
        },
        async mkdir() {},
        async delete() {},
        async rename() {},
        async writeFile() {},
    } as never);
}
