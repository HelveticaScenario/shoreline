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

/** The messages of the errors in `file` of a program holding `source`. */
function errors(source: string, file = 'patch.js'): string[] {
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
        .getPreEmitDiagnostics(program, program.getSourceFile(file))
        .map((d) => ts.flattenDiagnosticMessageText(d.messageText, '\n'));
}

test('chain modules and .wet type on outputs, collections and named outputs', () => {
    expect(
        errors(
            [
                `$saw('c3').lpf('800hz', 2).wet(2.5).out()`,
                `$c($sine('c3'), $sine('e3')).plate({ decay: 2 }).wet().out()`,
                `$saw('c3').unstable.shape.fold('dual', 2).wet(1).out()`,
                `$saw('c3').xover()._low.lpf('200hz').wet(2).out()`,
                `$saw('c3').xover().wet(2).out()`,
                `$saw('c3').delay(0.25, { feedback: 2 }).wet(2).out()`,
                `$saw($spread(0, 5, 3)).setLanes(2, { mode: 'average' }).out()`,
                `$saw('c3').addHz(2).mulHz(1.5).out()`,
                `$v.osc(8).hsv().kaleid(5).wet(0.5).out()`,
            ].join('\n'),
        ),
    ).toEqual([]);
});

test('a chained multi-output module keeps its outputs', () => {
    expect(errors(`$clock.tr808.hiHat()._open.out()`)).toEqual([]);
    expect(errors(`$clock.tr808.hiHat()._notThere`)).toHaveLength(1);
});

test('a pattern types as the module that plays it', () => {
    expect(
        errors(
            [
                `const seq = $p('c3 e3 g3')`,
                `$saw(seq).amp($perc(seq._trig)).lpf(seq.range(-1, 1, 'c4', 'c6')).out()`,
                `seq.fast(2).glide(0.1).saw().out()`,
                `seq[0].scope()`,
                `$p.s('0 2 4', 'c(maj)').trans.squeeze('0 7').add(1).out()`,
                `$p.arrange([2, seq], [1, seq.slow(2)]).mul(2).out()`,
                `$cycle(seq.struct('x ~ x x')).out()`,
            ].join('\n'),
        ),
    ).toEqual([]);
    // A lane, a slice or the receiver `.out()` returns is the sequencer's
    // signal, which has no pattern methods.
    expect(errors(`$p('c3').scope().fast(2)`)).toHaveLength(1);
    expect(errors(`$p('c3')[0].glide(0.1)`)).toHaveLength(1);
    expect(errors(`$cycle($p('c3').out())`)).toHaveLength(1);
});

test('the generated lib names only types it declares', () => {
    expect(
        errors('', 'dsl.d.ts').filter((message) =>
            message.startsWith('Cannot find name'),
        ),
    ).toEqual([]);
});

test('a name that is no module or method is an error', () => {
    expect(errors(`$saw('c3').notAModule().out()`)).toHaveLength(1);
});
