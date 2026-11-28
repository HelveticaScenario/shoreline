/**
 * Starts the VS Code API the editor runs on, in its classic mode: Monarch
 * highlighting and standalone themes, as plain Monaco has. No editor may be
 * created before `startMonaco` resolves.
 */
import { LogLevel } from '@codingame/monaco-vscode-api';
import * as css from '@codingame/monaco-vscode-standalone-css-language-features';
// Monaco's Monarch languages, which the VS Code API's editor leaves out.
import '@codingame/monaco-vscode-standalone-languages';
import * as json from '@codingame/monaco-vscode-standalone-json-language-features';
import * as typescript from '@codingame/monaco-vscode-standalone-typescript-language-features';
import * as editorApi from 'monaco-editor';
import {
    getEnhancedMonacoEnvironment,
    MonacoVscodeApiWrapper,
} from 'monaco-languageclient/vscodeApiWrapper';
import tsWorker from './components/monaco/dslTsWorker?worker';
// Local entries, so the dependency optimizer leaves the workers to Vite.
import cssWorker from './monacoWorkers/css?worker';
import editorWorker from './monacoWorkers/editor?worker';
import jsonWorker from './monacoWorkers/json?worker';
import type { Monaco } from './hooks/useCustomMonaco';
import { registerModelFileSystem } from './modelFileSystem';

function getWorker(_: string, label: string): Worker {
    if (label === 'typescript' || label === 'javascript') {
        return new tsWorker();
    }
    if (label === 'json') {
        return new jsonWorker();
    }
    if (label === 'css' || label === 'scss' || label === 'less') {
        return new cssWorker();
    }
    return new editorWorker();
}

export async function startMonaco(): Promise<Monaco> {
    const wrapper = new MonacoVscodeApiWrapper({
        $type: 'classic',
        logLevel: LogLevel.Warning,
        monacoWorkerFactory: () => {
            getEnhancedMonacoEnvironment().getWorker = getWorker;
        },
        viewsConfig: { $type: 'EditorService' },
    });
    await wrapper.start();
    registerModelFileSystem();
    // Prettier formats patches. A second formatter would have the editor ask
    // which to use, through a service this build leaves out.
    for (const defaults of [
        typescript.javascriptDefaults,
        typescript.typescriptDefaults,
    ]) {
        defaults.setModeConfiguration({
            ...defaults.modeConfiguration,
            documentRangeFormattingEdits: false,
            onTypeFormattingEdits: false,
        });
    }
    // The language namespaces live in their own packages; plain Monaco
    // exposes them on its root.
    return { ...editorApi, css, json, typescript };
}
