/**
 * The editor's signal-site collector, run against the generated DSL lib with
 * the compiler options the editor uses, and checked against what the runtime
 * records for the same source.
 */

import { describe, expect, test } from 'vitest';
import ts from 'typescript';
import schemas from '@modular/core/schemas.json';
import {
    collectSignalSites,
    siteOfRecord,
    type SignalSite,
} from '../../../shared/dsl/signalSites';
import { executePatchScript } from '../executor';
import { buildSignalSiteTables } from '../signalSiteTables';
import { buildLibSource } from '../typescriptLibGen';

const tables = buildSignalSiteTables(schemas);
const lib = buildLibSource(schemas);

const OPTIONS: ts.CompilerOptions = {
    allowJs: true,
    allowNonTsExtensions: true,
    checkJs: true,
    lib: ['lib.esnext.d.ts'],
    noEmit: true,
    target: ts.ScriptTarget.ES2020,
};

function collect(source: string): SignalSite[] {
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
    return collectSignalSites(
        ts,
        program.getSourceFile('patch.js')!,
        program.getTypeChecker(),
        tables,
    );
}

/** Each site as `<text before the anchor>‹output›`, in source order. */
function hints(source: string): string[] {
    return collect(source)
        .sort((a, b) => a.anchor - b.anchor)
        .map((site) => {
            const lineStart = source.lastIndexOf('\n', site.anchor - 1) + 1;
            const before = source.slice(lineStart, site.anchor).trim();
            return `${before.slice(-12)}‹${site.output ?? '?'}›`;
        });
}

describe('static resolution', () => {
    test('a root variable resolves per input', () => {
        expect(
            hints(`const seq = $p('c e g').c\n$saw(seq).amp($perc(seq)).out()`),
        ).toEqual(['$saw(seq‹cv›', 'mp($perc(seq‹trig›']);
    });

    test('a cycle resolves as argument, chain receiver and method receiver', () => {
        expect(
            hints(
                `const cyc = $p('0*8').cycle()\ncyc.$.perc()\n$adsr(cyc)\ncyc.amp(2).out()`,
            ),
        ).toEqual(['cyc‹trig›', '$adsr(cyc‹gate›', 'cyc‹cv›']);
    });

    test('an inline chain puts the hint after the receiver', () => {
        const source = `$p('0*8').c.$.perc().out()`;
        const [site] = collect(source);
        expect(source.slice(0, site.anchor)).toBe(`$p('0*8').c`);
        expect(site.output).toBe('trig');
    });

    test('the clock resolves by input', () => {
        expect(hints(`$perc($clock)\n$adsr($clock)\n$saw($clock)`)).toEqual([
            '$perc($clock‹barTrigger›',
            '$adsr($clock‹barTrigger›',
            '$saw($clock‹playhead›',
        ]);
    });

    test('an array built in a loop resolves by its element type', () => {
        expect(
            hints(
                `const seqs = []\nfor (let i = 0; i < 5; i++) {\n    seqs.push($p('c e g').c)\n}\n$sine(seqs).amp($perc(seqs)).out()`,
            ),
        ).toEqual(['$sine(seqs‹cv›', 'p($perc(seqs‹trig›']);
    });

    test('array literal elements resolve one by one', () => {
        expect(
            hints(`const a = $p('c').c\nconst b = $p('e').c\n$perc([a, b])`),
        ).toEqual(['$perc([a‹trig›', '$perc([a, b‹trig›']);
    });

    test('a config-object input resolves by its own type', () => {
        expect(
            hints(`const seq = $p('c e').cycle()\n$saw('c', { sync: seq })`),
        ).toEqual([' { sync: seq‹trig›']);
    });

    test('a chain method argument reads cv', () => {
        expect(
            hints(`const seq = $p('0 2 5').cycle()\n$sine('c').amp(seq)`),
        ).toEqual(["'c').amp(seq‹cv›"]);
    });

    test('indexing a root reads cv', () => {
        expect(hints(`const seq = $p('c e').cycle()\n$perc(seq[0])`)).toEqual([
            '$perc(seq‹cv›',
        ]);
    });

    test('named outputs, plain signals and pattern params get no site', () => {
        expect(
            hints(
                `const seq = $p('c e g').cycle()\n$saw(seq.cv).amp($perc(seq.trig)).out()\n$sine('c').amp(2)\n$cycle($p('c'))`,
            ),
        ).toEqual([]);
    });

    test('an untyped value is left for the runtime', () => {
        expect(
            hints(`function voice(s) {\n    return $saw(s).amp($perc(s))\n}`),
        ).toEqual(['eturn $saw(s‹?›', '.amp($perc(s‹?›']);
    });
});

describe('agreement with the runtime', () => {
    const sources = [
        `const seq = $p('c e g').c\n$saw(seq).amp($perc(seq)).out()`,
        `$saw($p('c e g').c).amp($perc($p('c*4').c)).out()`,
        `const cyc = $p('0*8').cycle()\ncyc.$.perc().out()\n$adsr(cyc).out()\ncyc.amp(2).out()`,
        `$p('0*8').cycle().$.adsr().out()`,
        `const seqs = []\nfor (let i = 0; i < 3; i++) {\n    seqs.push($p('c e g').c)\n}\n$sine(seqs).amp($perc(seqs)).out()`,
        `const a = $p('c').c\nconst b = $p('e').c\n$perc([a, b]).out()`,
        `const seq = $p('c e').cycle()\n$saw(seq, { sync: seq }).out()`,
        `function voice(s) {\n    return $saw(s).amp($perc(s))\n}\nvoice($p('c e g').c).out()`,
        `$perc($clock).out()\n$tr808.kick($clock).out()`,
        `const seq = $p('c e').cycle()\nseq.out()\n$sine('c').amp(seq).out()`,
        `$sine('c').$.lpf($p('c5 c6').c).out()`,
    ];

    test.each(sources)('%s', (source) => {
        const sites = collect(source);
        const { outputResolutions } = executePatchScript(source, schemas, {
            sampleRate: 48_000,
            workspaceRoot: '/workspace',
        });
        expect(outputResolutions.length).toBeGreaterThan(0);
        for (const record of outputResolutions) {
            const site = siteOfRecord(sites, record);
            expect(site, JSON.stringify(record)).toBeDefined();
            if (site!.output !== null) {
                expect(site!.output, JSON.stringify(record)).toBe(
                    record.output,
                );
            }
        }
        // Every site is either resolved statically or filled by a record.
        for (const site of sites.filter((s) => s.output === null)) {
            expect(
                outputResolutions.some((r) => siteOfRecord(sites, r) === site),
                JSON.stringify(site),
            ).toBe(true);
        }
    });
});
