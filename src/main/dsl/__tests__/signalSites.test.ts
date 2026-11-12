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
        .sort((a, b) => a.end - b.end)
        .map((site) => {
            const lineStart = source.lastIndexOf('\n', site.end - 1) + 1;
            const before = source.slice(lineStart, site.end).trim();
            return `${before.slice(-12)}‹${site.output ?? '?'}›`;
        });
}

describe('static resolution', () => {
    test('a root variable resolves per input', () => {
        expect(
            hints(`const seq = $p('c e g')\n$saw(seq).amp($perc(seq)).out()`),
        ).toEqual(['$saw(seq‹cv›', 'mp($perc(seq‹trig›']);
    });

    test('a cycle resolves as argument, chain receiver and method receiver', () => {
        expect(
            hints(
                `const cyc = $p('0*8')\ncyc.perc()\n$adsr(cyc)\ncyc.amp(2).out()`,
            ),
        ).toEqual(['cyc‹trig›', '$adsr(cyc‹gate›', 'cyc‹cv›']);
    });

    test('an inline chain puts the hint after the receiver', () => {
        const source = `$p('0*8').perc().out()`;
        const [site] = collect(source);
        expect(source.slice(0, site.end)).toBe(`$p('0*8')`);
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
                `const seqs = []\nfor (let i = 0; i < 5; i++) {\n    seqs.push($p('c e g'))\n}\n$sine(seqs).amp($perc(seqs)).out()`,
            ),
        ).toEqual(['$sine(seqs‹cv›', 'p($perc(seqs‹trig›']);
    });

    test('array literal elements resolve one by one', () => {
        expect(
            hints(`const a = $p('c')\nconst b = $p('e')\n$perc([a, b])`),
        ).toEqual(['$perc([a‹trig›', '$perc([a, b‹trig›']);
    });

    test('a config-object input resolves by its own type', () => {
        expect(
            hints(`const seq = $p('c e')\n$saw('c', { sync: seq })`),
        ).toEqual([' { sync: seq‹trig›']);
    });

    test('a chain method argument reads cv', () => {
        expect(hints(`const seq = $p('0 2 5')\n$sine('c').amp(seq)`)).toEqual([
            "'c').amp(seq‹cv›",
        ]);
    });

    test('a channel of a root resolves per input', () => {
        expect(
            hints(`const seq = $p('c e')\n$saw(seq[0]).amp($perc(seq[0]))`),
        ).toEqual(['$saw(seq[0]‹cv›', '$perc(seq[0]‹trig›']);
    });

    test('named outputs, plain signals and pattern params get no site', () => {
        expect(
            hints(
                `const seq = $p('c e g')\n$saw(seq._cv).amp($perc(seq._trig)).out()\n$sine('c').amp(2)\n$cycle($p('c'))`,
            ),
        ).toEqual([]);
    });

    test('a signal group resolves the values inside it', () => {
        expect(hints(`const seq = $p('c e')\n$perc($g1([seq, seq]))`)).toEqual([
            'erc($g1([seq‹trig›',
            'g1([seq, seq‹trig›',
        ]);
    });

    test('a mix amount reads cv', () => {
        expect(
            hints(
                `const seq = $p('1 2')\n$sine('c').lpf('c').wet(seq)\n$sine('c').pipeMix((x) => x, seq)`,
            ),
        ).toEqual(["'c').wet(seq‹cv›", 'x) => x, seq‹cv›']);
    });

    test('a chain method option resolves by its own class', () => {
        expect(
            hints(
                `const seq = $p('c e')\n$sine('c').erratic(1, { trigger: seq, speed: seq }).out({ gain: seq })`,
            ),
        ).toEqual([
            'trigger: seq‹trig›',
            ', speed: seq‹cv›',
            '({ gain: seq‹cv›',
        ]);
    });

    test('$delay resolves its signals as a call and as a chain', () => {
        expect(
            hints(
                `const seq = $p('1 2')\n$delay(seq, seq, { feedback: seq })\n$sine('c').delay(seq)`,
            ),
        ).toEqual([
            '$delay(seq‹cv›',
            'lay(seq, seq‹cv›',
            'eedback: seq‹cv›',
            "').delay(seq‹cv›",
        ]);
    });

    test('only a chain method reads its receiver', () => {
        expect(
            hints(`const seq = $p('c e')\nseq.length\nseq.toString()`),
        ).toEqual([]);
    });

    test('only a value a member can follow is insertable', () => {
        const sites = collect(
            `const seq = $p('c e')\nconst seqs = [seq, seq]\n$perc(seq)\n$perc(seqs)\n$perc(true ? seq : seq)\n$perc((seq))`,
        );
        expect(sites.map((site) => site.insertable)).toEqual([
            true,
            false,
            false,
            true,
        ]);
    });

    test('a shorthand property is not insertable', () => {
        const sites = collect(
            `const trigger = $p('c e')\n$perc(0, { trigger })`,
        );
        expect(sites.map((site) => site.insertable)).toEqual([false]);
    });

    test('a slice or chunk of a root resolves per input', () => {
        expect(
            hints(
                `const seq = $p('c e')\n$perc(seq.slice(0, 1))\n$saw(seq.chunk(1))`,
            ),
        ).toEqual(['.slice(0, 1)‹trig›', 'seq.chunk(1)‹cv›']);
    });

    test('arguments after a spread are not placed', () => {
        expect(
            hints(`const seq = $p('c e')\n$saw(...[seq], { sync: seq })`),
        ).toEqual([]);
    });

    test('a name shared with Object.prototype is not a root or a module', () => {
        expect(
            hints(
                `const seq = $p('c e')\n$perc(seq.toString)\nfunction valueOf() {}\nvalueOf[0]\ntoString(seq)`,
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
        `const seq = $p('c e g')\n$saw(seq).amp($perc(seq)).out()`,
        `$saw($p('c e g')).amp($perc($p('c*4'))).out()`,
        `const cyc = $p('0*8')\ncyc.perc().out()\n$adsr(cyc).out()\ncyc.amp(2).out()`,
        `$p('0*8').adsr().out()`,
        `const seqs = []\nfor (let i = 0; i < 3; i++) {\n    seqs.push($p('c e g'))\n}\n$sine(seqs).amp($perc(seqs)).out()`,
        `const a = $p('c')\nconst b = $p('e')\n$perc([a, b]).out()`,
        `const seq = $p('c e')\n$saw(seq, { sync: seq }).out()`,
        `function voice(s) {\n    return $saw(s).amp($perc(s))\n}\nvoice($p('c e g')).out()`,
        `$perc($clock).out()\n$tr808.kick($clock).out()`,
        `const seq = $p('c e')\nseq.out()\n$sine('c').amp(seq).out()`,
        `$sine('c').lpf($p('c5 c6')).out()`,
        `const seq = $p('c e')\n$saw($g1([seq, seq]), { sync: $g2(seq) }).out()`,
        `function f(s, m) {\n    return s.xover().wet(m)\n}\nf($clock, $p('1 2'))`,
        `function f(m) {\n    return $sine('c').pipeMix((x) => x.lpf('c'), m)\n}\nf($p('1 2')).out()`,
        `function f(s) {\n    return $sine('c').range(s, s)\n}\nf($p('0 1')).out()`,
        `function f(s) {\n    return $sine('c').amp($g1([s, s])).out({ gain: $g1(s) })\n}\nf($p('0 1'))`,
        `function f(s) {\n    return $mix([[s, 2.5]]).amp($step([s, 'c'], s))\n}\nf($p('0 1')).out()`,
        `function f(s) {\n    return $quantizer($sine('c'), s)\n}\nf($p('c, e, g')).out()`,
        `function f(s) {\n    return $sine('c').erratic(1, { trigger: s, speed: s })\n}\nf($p('0 1')).out({ gain: $p('1 2') })`,
        `function f(s) {\n    return $delay(s, s, { feedback: s, feedbackCb: (m) => m.lpf(s) })\n}\nf($p('0 1'))`,
        `function f(s) {\n    return $sine('c').delay(s).wet(s)\n}\nf($p('0 1'))`,
        `const seq = $p('[c,e] [g,b]')\n$perc(seq.slice(1)).out()\n$saw(seq.chunk(1)).amp($perc(seq[1])).out()\nseq.slice(1).perc().out()`,
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
