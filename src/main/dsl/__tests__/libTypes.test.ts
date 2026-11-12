/**
 * The generated DSL lib declares the collection types cleanly and types
 * patches the way the editor shows them, with the compiler options the editor
 * uses.
 */

import { describe, expect, test } from 'vitest';
import ts from 'typescript';
import schemas from '@modular/core/schemas.json';
import { buildLibSource } from '../typescriptLibGen';

const LIB_FILE = '/modular/dsl-lib.d.ts';
const PATCH_FILE = '/modular/patch.js';

const libSource = buildLibSource(schemas);

/** The diagnostics for `patch` against the DSL lib, as `file:line: message`. */
function diagnostics(patch: string): string[] {
    const options: ts.CompilerOptions = {
        allowJs: true,
        allowNonTsExtensions: true,
        checkJs: true,
        lib: ['lib.esnext.d.ts'],
        module: ts.ModuleKind.ESNext,
        moduleResolution: ts.ModuleResolutionKind.Node10,
        noEmit: true,
        target: ts.ScriptTarget.ES2020,
    };
    const files = new Map([
        [LIB_FILE, libSource],
        [PATCH_FILE, patch],
    ]);
    const base = ts.createCompilerHost(options);
    const host: ts.CompilerHost = {
        ...base,
        fileExists: (name) => files.has(name) || base.fileExists(name),
        getSourceFile: (name, language, ...rest) => {
            const text = files.get(name);
            return text === undefined
                ? base.getSourceFile(name, language, ...rest)
                : ts.createSourceFile(name, text, language);
        },
        readFile: (name) => files.get(name) ?? base.readFile(name),
    };
    const program = ts.createProgram([LIB_FILE, PATCH_FILE], options, host);
    return ts.getPreEmitDiagnostics(program).map((d) => {
        const line = d.file
            ? `${d.file.fileName}:${d.file.getLineAndCharacterOfPosition(d.start ?? 0).line + 1}`
            : '?';
        return `${line}: ${ts.flattenDiagnosticMessageText(d.messageText, '\n')}`;
    });
}

describe('generated DSL lib', () => {
    test('declares the collection types cleanly', () => {
        expect(libSource).not.toMatch(/^(<{7}|\|{7}|={7}|>{7})/m);
        expect(libSource).not.toMatch(/\bModuleOutput/);
        const collectionErrors = diagnostics('').filter((d) =>
            /Collection|OutLane|Duplicate identifier|Cannot extend/.test(d),
        );
        expect(collectionErrors).toEqual([]);
    });

    test('collections index and iterate as one-channel collections', () => {
        const patchErrors = diagnostics(`
                const [a, b] = $saw(['c3', 'e3']);
                a.amp(2).out();
                b[0][0].add(1).out();
                for (const voice of $sine(['c3', 'g3'])) voice.scope();
                $saw([a, b[0]]).out();
                $r($sine('1hz'), $sine('2hz')[0]).range(0, 1);
                $deferred(2)[1].set($sine('c3'));
                $sine($clock._tempo.div(60)).out();
                const lane = $outLane('bass');
                lane[0].level.range(0, 1);
                for (const c of lane) c.pan.range(-1, 1);
                const seq = $p('[c4,e4] [g4,b4]');
                $perc(seq[1]).out();
                $perc(seq[1]._trig).out();
                for (const voice of seq) voice._gate.scope();
            `).filter((d) => d.startsWith(PATCH_FILE));
        expect(patchErrors).toEqual([]);
    });

    test('arithmetic chains off collections, ranged collections and patterns', () => {
        const patchErrors = diagnostics(`
                $sine('c3').add(1).sub(0.5).mul(2).div(4).exp(2).out();
                $sine('1hz').range(0, 1).mul($p('1 2')).out();
                $p('0 2 4').add(1).out();
            `).filter((d) => d.startsWith(PATCH_FILE));
        expect(patchErrors).toEqual([]);
    });

    test('an arithmetic method needs its operand', () => {
        const patchErrors = diagnostics(`$sine('c3').add().out();`).filter(
            (d) => d.startsWith(PATCH_FILE),
        );
        expect(patchErrors).toHaveLength(1);
    });
});
