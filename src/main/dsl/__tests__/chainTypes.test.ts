/**
 * The generated DSL lib types chain modules and `.wet` on outputs and
 * collections, checked with the compiler options the editor uses.
 */

import { expect, test } from 'vitest';
import ts from 'typescript';
import schemas from '@modular/core/schemas.json';
import { buildLibSource } from '../typescriptLibGen';

const OPTIONS: ts.CompilerOptions = {
    allowJs: true,
    allowNonTsExtensions: true,
    checkJs: true,
    lib: ['lib.esnext.d.ts'],
    noEmit: true,
    target: ts.ScriptTarget.ES2020,
};

const lib = buildLibSource(schemas);

/** The messages of the errors the editor would show in `source`. */
function errors(source: string): string[] {
    const files: Record<string, string> = {
        'dsl.d.ts': lib,
        'patch.js': source,
    };
    const host = ts.createCompilerHost(OPTIONS);
    const readDefault = host.getSourceFile.bind(host);
    host.getSourceFile = (name, version) =>
        name in files
            ? ts.createSourceFile(name, files[name], version, true)
            : readDefault(name, version);
    host.fileExists = (name) => name in files || ts.sys.fileExists(name);
    host.readFile = (name) => files[name] ?? ts.sys.readFile(name);
    const program = ts.createProgram(Object.keys(files), OPTIONS, host);
    return ts
        .getPreEmitDiagnostics(program, program.getSourceFile('patch.js'))
        .map((d) => ts.flattenDiagnosticMessageText(d.messageText, '\n'));
}

test('chain modules and .wet type on outputs, collections and named outputs', () => {
    expect(
        errors(
            [
                `$saw('c3').lpf('800hz', 2).wet(2.5).out()`,
                `$c($sine('c3'), $sine('e3')).plate({ decay: 2 }).wet().out()`,
                `$saw('c3').unstable.shape.fold('dual', 2).wet(1).out()`,
                `$saw('c3').xover().o.low.lpf('200hz').wet(2).out()`,
                `$saw('c3').xover().wet(2).out()`,
                `$saw('c3').delay(0.25, { feedback: 2 }).wet(2).out()`,
                `$saw($spread(0, 5, 3)).mixDown(2, { mode: 'average' }).out()`,
                `$saw('c3').addHz(2).mulHz(1.5).out()`,
                `$v.osc(8).hsv().kaleid(5).wet(0.5).out()`,
            ].join('\n'),
        ),
    ).toEqual([]);
});

test('a name that is no module or method is an error', () => {
    expect(errors(`$saw('c3').notAModule().out()`)).toHaveLength(1);
});
