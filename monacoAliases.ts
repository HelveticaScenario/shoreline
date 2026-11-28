import path from 'path';
import type { Alias } from 'vite';

const modules = path.resolve(__dirname, 'node_modules/@codingame');
const ts = `${modules}/monaco-vscode-standalone-typescript-language-features`;

/**
 * Deep `monaco-editor` imports, resolved inside the VS Code API's build of the
 * editor (the package `monaco-editor` names). Its internals sit at the same
 * `vs/` paths, the TypeScript worker's in its own package, and the editor
 * worker is the one export the package keeps. Ordered: the first match wins.
 */
export const monacoAliases: Alias[] = [
    {
        find: /^monaco-editor\/esm\/vs\/language\/typescript\/(.*)$/,
        replacement: `${ts}/languages/features/typescript/$1`,
    },
    {
        find: /^monaco-editor\/esm\/vs\/common\/initialize\.js$/,
        replacement: `${ts}/internal/common/initialize.js`,
    },
    {
        find: /^monaco-editor\/esm\/(?!vs\/editor\/editor\.worker)(vs\/.*)\.js$/,
        replacement: '@codingame/monaco-vscode-api/vscode/$1',
    },
];
