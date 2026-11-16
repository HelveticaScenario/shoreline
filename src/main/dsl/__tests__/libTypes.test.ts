/**
 * The generated DSL lib declares the collection types cleanly and types
 * patches the way the editor shows them, with the compiler options the editor
 * uses.
 */

import { describe, expect, test } from 'vitest';
import ts from 'typescript';
import schemas from '@modular/core/schemas.json';
import { buildLibSource } from '../typescriptLibGen';
import { trackLabelView } from '../../../renderer/components/monaco/trackLabelBindings';

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
            /Collection|TrackTap|Duplicate identifier|Cannot extend/.test(d),
        );
        expect(collectionErrors).toEqual([]);
    });

    test('collections index and iterate as one-lane collections', () => {
        const patchErrors = diagnostics(`
                const [a, b] = $saw(['c3', 'e3']);
                out: a.amp(2);
                out2: b[0][0].add(1);
                for (const voice of $sine(['c3', 'g3'])) voice.scope();
                out3: $saw([a, b[0]]);
                $r($sine('1hz'), $sine('2hz')[0]).range(0, 1);
                $deferred(2)[1].set($sine('c3'));
                out4: $sine($clock._tempo.div(60));
                const tap = $trackTap('bass');
                tap[0].level.range(0, 1);
                for (const c of tap) c.panning.range(-1, 1);
                const seq = $('[c4,e4] [g4,b4]');
                out5: $perc(seq[1]);
                out6: $perc(seq[1]._trig);
                for (const voice of seq) voice._gate.scope();
            `).filter((d) => d.startsWith(PATCH_FILE));
        expect(patchErrors).toEqual([]);
    });

    test('arithmetic chains off collections, ranged collections and patterns', () => {
        const patchErrors = diagnostics(`
                out: $sine('c3').add(1).sub(0.5).mul(2).div(4).exp(2);
                out2: $sine('1hz').range(0, 1).mul($('1 2'));
                out3: $('0 2 4').add(1);
            `).filter((d) => d.startsWith(PATCH_FILE));
        expect(patchErrors).toEqual([]);
    });

    test('concat joins collections of one type and keeps that type', () => {
        const patchErrors = diagnostics(`
                out: $sine('c3').concat($saw('e3'));
                $sine('1hz').range(0, 1).concat($sine('2hz')).range(0, 5);
                const oscs = $enosc('c3').concat($enosc('g3'));
                out2: oscs._all;
                out3: $saw(oscs);
                const seq = $('c4 e4').concat($('g3 b3'));
                out4: $perc(seq._trig);
                seq.concat($('c5'))._gate.scope();
                const tap = $trackTap('bass').concat($trackTap('lead'));
                tap.level.range(0, 1);
            `).filter((d) => d.startsWith(PATCH_FILE));
        expect(patchErrors).toEqual([]);
    });

    test('concat rejects a collection of another type', () => {
        const patchErrors = diagnostics(`
                $enosc('c3').concat($sine('c3'));
                $('c4').concat($enosc('c3'));
            `).filter((d) => d.startsWith(PATCH_FILE));
        expect(patchErrors).toHaveLength(2);
    });

    test('an arithmetic method needs its operand', () => {
        const patchErrors = diagnostics(`out: $sine('c3').add();`).filter((d) =>
            d.startsWith(PATCH_FILE),
        );
        expect(patchErrors).toHaveLength(1);
    });
});

describe('track labels in the editor', () => {
    /** Patch errors with the label bindings the editor's worker appends. */
    const labelErrors = (patch: string): string[] => {
        const lines = patch.split('\n').length;
        return diagnostics(trackLabelView(ts, patch).text).filter((d) => {
            const at = /^\/modular\/patch\.js:(\d+):/.exec(d);
            return at !== null && Number(at[1]) <= lines;
        });
    };

    test("a bus's name takes sends and a signal's name is its tap", () => {
        expect(
            labelErrors(`
verb: $bus((m) => m.plate()).track({ gain: 3 })
kick: $sine('c1')
bass: $saw('c2').amp(kick.active).track({ sends: [verb, 2] })
lead: $saw('c3').track({ pan: kick.panning, sends: verb }).scope()
kick.level.range(0, 1)
`),
        ).toEqual([]);
    });

    test('a name reads before its label', () => {
        expect(
            labelErrors(`
bass: $saw('c2').amp(kick)
kick: $sine('c1')
`),
        ).toEqual([]);
    });

    test('only a bus is a send target', () => {
        expect(
            labelErrors(`
kick: $sine('c1')
lead: $saw('c3').track({ sends: kick })
`),
        ).toHaveLength(1);
    });

    test('a track has no chain methods but scope', () => {
        expect(labelErrors(`lead: $saw('c3').track().lpf('c4')`)).toHaveLength(
            1,
        );
    });

    test('labels are not reported as unused', () => {
        expect(labelErrors(`lead: $saw('c3')`)).toEqual([]);
    });
});
