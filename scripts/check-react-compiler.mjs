// Fails when React Compiler skips a renderer component or hook. The compiler
// leaves a function it cannot handle uncompiled without failing the build, so
// this is the only signal that one has lost its memoization. A function
// opts out on purpose with a 'use no memo' directive.
import { transformSync } from '@babel/core';
import fs from 'node:fs';
import path from 'node:path';

const ROOT = 'src/renderer';

function sourceFiles(dir) {
    return fs.readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
        const file = path.join(dir, entry.name);
        if (entry.isDirectory()) {
            return entry.name === '__tests__' ? [] : sourceFiles(file);
        }
        return /\.tsx?$/.test(entry.name) &&
            !/\.(test|d)\.tsx?$/.test(entry.name)
            ? [file]
            : [];
    });
}

let compiled = 0;
const skipped = [];
for (const file of sourceFiles(ROOT)) {
    const source = fs.readFileSync(file, 'utf8');
    const lines = source.split('\n');
    const events = [];
    transformSync(source, {
        babelrc: false,
        configFile: false,
        filename: file,
        parserOpts: { plugins: ['typescript', 'jsx'] },
        plugins: [
            [
                'babel-plugin-react-compiler',
                { logger: { logEvent: (_file, event) => events.push(event) } },
            ],
        ],
    });
    for (const event of events) {
        if (event.kind === 'CompileSuccess') {
            compiled++;
        } else if (
            event.kind === 'CompileError' ||
            event.kind === 'PipelineError'
        ) {
            const { start, end } = event.fnLoc ?? {};
            const body = start
                ? lines.slice(start.line - 1, end.line).join('\n')
                : '';
            if (/['"]use no memo['"]/.test(body)) {
                continue;
            }
            const detail = event.detail?.options ?? event.detail ?? {};
            const at = detail.details?.[0]?.loc ?? detail.loc;
            skipped.push(
                `${file}:${at?.start?.line ?? start?.line ?? '?'}  ${detail.reason ?? event.data ?? 'not compiled'}`,
            );
        }
    }
}

if (skipped.length > 0) {
    console.error(
        `React Compiler skipped ${skipped.length} function(s):\n  ${skipped.join('\n  ')}`,
    );
    process.exit(1);
}
console.log(`React Compiler compiled ${compiled} components and hooks.`);
