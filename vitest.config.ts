import { defineConfig } from 'vitest/config';
import path from 'path';
import { monacoAliases } from './monacoAliases';

export default defineConfig({
    test: {
        include: [
            'src/**/*.test.ts',
            'src/**/*.test.tsx',
            'crates/modular/__test__/**/*.test.ts',
        ],
        // Native N-API modules don't work with worker threads
        pool: 'forks',
        forks: {
            singleFork: true,
        },
        testTimeout: 30_000,
        // Transformed rather than loaded by Node, so resetting modules gives a
        // test fresh copies of the editor's process-wide singletons.
        server: { deps: { inline: [/@codingame\/monaco-vscode-api/] } },
    },
    resolve: {
        alias: [
            ...monacoAliases,
            {
                find: '@modular/core',
                replacement: path.resolve(__dirname, 'crates/modular'),
            },
        ],
    },
});
