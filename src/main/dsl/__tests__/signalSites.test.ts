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

function collect(
    source: string,
    collector: typeof collectSignalSites = collectSignalSites,
): SignalSite[] {
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
    return collector(
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
            hints(`const seq = $('c e g')\n$saw(seq).amp($perc(seq)).out()`),
        ).toEqual(['$saw(seq‹cv›', 'mp($perc(seq‹trig›']);
    });

    test('a cycle resolves as argument, chain receiver and method receiver', () => {
        expect(
            hints(
                `const cyc = $('0*8')\ncyc.perc()\n$adsr(cyc)\ncyc.amp(2).out()`,
            ),
        ).toEqual(['cyc‹trig›', '$adsr(cyc‹gate›', 'cyc‹cv›']);
    });

    test('an inline chain puts the hint after the receiver', () => {
        const source = `$('0*8').perc().out()`;
        const [site] = collect(source);
        expect(source.slice(0, site.end)).toBe(`$('0*8')`);
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
                `const seqs = []\nfor (let i = 0; i < 5; i++) {\n    seqs.push($('c e g'))\n}\n$sine(seqs).amp($perc(seqs)).out()`,
            ),
        ).toEqual(['$sine(seqs‹cv›', 'p($perc(seqs‹trig›']);
    });

    test('array literal elements resolve one by one', () => {
        expect(
            hints(`const a = $('c')\nconst b = $('e')\n$perc([a, b])`),
        ).toEqual(['$perc([a‹trig›', '$perc([a, b‹trig›']);
    });

    test('a config-object input resolves by its own type', () => {
        expect(hints(`const seq = $('c e')\n$saw('c', { sync: seq })`)).toEqual(
            [' { sync: seq‹trig›'],
        );
    });

    test('a chain method argument reads cv', () => {
        expect(hints(`const seq = $('0 2 5')\n$sine('c').amp(seq)`)).toEqual([
            "'c').amp(seq‹cv›",
        ]);
    });

    test('a lane of a root resolves per input', () => {
        expect(
            hints(`const seq = $('c e')\n$saw(seq[0]).amp($perc(seq[0]))`),
        ).toEqual(['$saw(seq[0]‹cv›', '$perc(seq[0]‹trig›']);
    });

    test('named outputs, plain signals and pattern params get no site', () => {
        expect(
            hints(
                `const seq = $('c e g')\n$saw(seq._cv).amp($perc(seq._trig)).out()\n$sine('c').amp(2)\n$cycle($('c'))`,
            ),
        ).toEqual([]);
    });

    test('a signal group resolves the values inside it', () => {
        expect(hints(`const seq = $('c e')\n$perc($g1([seq, seq]))`)).toEqual([
            'erc($g1([seq‹trig›',
            'g1([seq, seq‹trig›',
        ]);
    });

    test('a mix amount reads cv', () => {
        expect(
            hints(
                `const seq = $('1 2')\n$sine('c').lpf('c').wet(seq)\n$sine('c').pipeMix((x) => x, seq)`,
            ),
        ).toEqual(["'c').wet(seq‹cv›", 'x) => x, seq‹cv›']);
    });

    test('a chain method option resolves by its own class', () => {
        expect(
            hints(
                `const seq = $('c e')\n$sine('c').erratic(1, { trigger: seq, speed: seq }).out({ gain: seq })`,
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
                `const seq = $('1 2')\n$delay(seq, seq, { feedback: seq })\n$sine('c').delay(seq)`,
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
            hints(`const seq = $('c e')\nseq.length\nseq.toString()`),
        ).toEqual([]);
    });

    test('only a value a member can follow is insertable', () => {
        const sites = collect(
            `const seq = $('c e')\nconst seqs = [seq, seq]\n$perc(seq)\n$perc(seqs)\n$perc(true ? seq : seq)\n$perc((seq))`,
        );
        expect(sites.map((site) => site.insertable)).toEqual([
            true,
            false,
            false,
            true,
        ]);
    });

    test('a tagged $ template is insertable', () => {
        const sites = collect('$perc($`c e`)');
        expect(sites.map((site) => site.insertable)).toEqual([true]);
    });

    test('a shorthand property is not insertable', () => {
        const sites = collect(
            `const trigger = $('c e')\n$perc(0, { trigger })`,
        );
        expect(sites.map((site) => site.insertable)).toEqual([false]);
    });

    test('a slice or chunk of a root resolves per input', () => {
        expect(
            hints(
                `const seq = $('c e')\n$perc(seq.slice(0, 1))\n$saw(seq.chunk(1))`,
            ),
        ).toEqual(['.slice(0, 1)‹trig›', 'seq.chunk(1)‹cv›']);
    });

    test('arguments after a spread are not placed', () => {
        expect(
            hints(`const seq = $('c e')\n$saw(...[seq], { sync: seq })`),
        ).toEqual([]);
    });

    test('a name shared with Object.prototype is not a root or a module', () => {
        expect(
            hints(
                `const seq = $('c e')\n$perc(seq.toString)\nfunction valueOf() {}\nvalueOf[0]\ntoString(seq)`,
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
        `const seq = $('c e g')\n$saw(seq).amp($perc(seq)).out()`,
        `$saw($('c e g')).amp($perc($('c*4'))).out()`,
        `const cyc = $('0*8')\ncyc.perc().out()\n$adsr(cyc).out()\ncyc.amp(2).out()`,
        `$('0*8').adsr().out()`,
        `const seqs = []\nfor (let i = 0; i < 3; i++) {\n    seqs.push($('c e g'))\n}\n$sine(seqs).amp($perc(seqs)).out()`,
        `const a = $('c')\nconst b = $('e')\n$perc([a, b]).out()`,
        `const seq = $('c e')\n$saw(seq, { sync: seq }).out()`,
        `function voice(s) {\n    return $saw(s).amp($perc(s))\n}\nvoice($('c e g')).out()`,
        `$perc($clock).out()\n$tr808.kick($clock).out()`,
        `const seq = $('c e')\nseq.out()\n$sine('c').amp(seq).out()`,
        `$sine('c').lpf($('c5 c6')).out()`,
        `const seq = $('c e')\n$saw($g1([seq, seq]), { sync: $g2(seq) }).out()`,
        `function f(s, m) {\n    return s.xover().wet(m)\n}\nf($clock, $('1 2'))`,
        `function f(m) {\n    return $sine('c').pipeMix((x) => x.lpf('c'), m)\n}\nf($('1 2')).out()`,
        `function f(s) {\n    return $sine('c').range(s, s)\n}\nf($('0 1')).out()`,
        `function f(s) {\n    return $sine('c').amp($g1([s, s])).out({ gain: $g1(s) })\n}\nf($('0 1'))`,
        `function f(s) {\n    return $mix([[s, 2.5]]).amp($step([s, 'c'], s))\n}\nf($('0 1')).out()`,
        `function f(s) {\n    return $quantizer($sine('c'), s)\n}\nf($('c, e, g')).out()`,
        `function f(s) {\n    return $sine('c').erratic(1, { trigger: s, speed: s })\n}\nf($('0 1')).out({ gain: $('1 2') })`,
        `function f(s) {\n    return $delay(s, s, { feedback: s, feedbackCb: (m) => m.lpf(s) })\n}\nf($('0 1'))`,
        `function f(s) {\n    return $sine('c').delay(s).wet(s)\n}\nf($('0 1'))`,
        `const seq = $('[c,e] [g,b]')\n$perc(seq.slice(1)).out()\n$saw(seq.chunk(1)).amp($perc(seq[1])).out()\nseq.slice(1).perc().out()`,
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

describe('fold sites', () => {
    /** Each fold of `source` as `<value>‹<hint>›`, in source order. */
    function folds(source: string): string[] {
        return collect(source)
            .filter((site) => site.fold)
            .map(
                (site) =>
                    `${source.slice(site.start, site.end)}‹${site.output ? `._${site.output}` : ''}.mono()›`,
            );
    }

    const wide = `const wide = $stereo($saw('c3'), $saw('g3'))\n`;

    test('a stereo value into an input that folds', () => {
        expect(folds(`${wide}$enosc(wide).out()`)).toEqual(['wide‹.mono()›']);
        expect(
            folds(`${wide}$plate($saw('c3'), { decay: wide }).out()`),
        ).toEqual(['wide‹.mono()›']);
        expect(folds(`${wide}wide.enosc().out()`)).toEqual(['wide‹.mono()›']);
    });

    test('a mono out folds what it sends', () => {
        expect(folds(`$stereo(0, 1).outMono()`)).toEqual([
            '$stereo(0, 1)‹.mono()›',
        ]);
        expect(folds(`${wide}wide.lpf('c5').outMono(0)`)).toEqual([
            "wide.lpf('c5')‹.mono()›",
        ]);
    });

    test('a module root folds through the output it reads', () => {
        expect(folds(`$coinFlip($stereo('c', 'g')).outMono()`)).toEqual([
            "$coinFlip($stereo('c', 'g'))‹._heads.mono()›",
        ]);
    });

    test('an input read side by side does not fold', () => {
        expect(folds(`${wide}$pan(wide, 2).out()`)).toEqual([]);
        expect(folds(`${wide}$mono(wide).outMono()`)).toEqual([]);
        expect(folds(`${wide}wide.mono().outMono()`)).toEqual([]);
    });

    test('a lifted module carries what its inputs carry', () => {
        expect(folds(`${wide}$enosc(wide.lpf('c5').add(1)).out()`)).toEqual([
            "wide.lpf('c5').add(1)‹.mono()›",
        ]);
        expect(folds(`$saw('c3').lpf('c5').outMono()`)).toEqual([]);
        expect(folds(`$enosc($saw($sine('1hz'))).out()`)).toEqual([]);
    });

    test('plain values, patterns and controls are mono', () => {
        expect(folds(`$enosc('c3').out()`)).toEqual([]);
        expect(folds(`$enosc($('c3 e3')).out()`)).toEqual([]);
        expect(folds(`const n = 3\n$enosc(n).out()`)).toEqual([]);
        expect(folds(`$saw($clock._tempo).outMono()`)).toEqual([]);
    });

    test('a buffer reader carries what its buffer records', () => {
        expect(
            folds(`${wide}$enosc($delayRead($buffer(wide, 1), 0.1)).out()`),
        ).toEqual(['$delayRead($buffer(wide, 1), 0.1)‹.mono()›']);
        expect(
            folds(
                `const b = $buffer($saw('c3'), 1)\n$enosc($delayRead(b, 0.1)).out()`,
            ),
        ).toEqual([]);
    });

    test('patterns and missing values are mono', () => {
        expect(folds(`$enosc($('c3 e3').fast(2).slow(3)).out()`)).toEqual([]);
        expect(folds(`$saw('c3').outMono(0, undefined)`)).toEqual([]);
    });

    test('a range lifts with its bounds', () => {
        expect(
            folds(`$enosc($sine('1hz').range($stereo(0, 1), 5)).out()`),
        ).toEqual(["$sine('1hz').range($stereo(0, 1), 5)‹.mono()›"]);
        expect(folds(`$enosc($sine('1hz').range(0, 5)).out()`)).toEqual([]);
    });

    test("an out's folding options", () => {
        expect(folds(`${wide}$saw('c3').out({ pan: wide })`)).toEqual([
            'wide‹.mono()›',
        ]);
        expect(folds(`${wide}$saw('c3').out({ gain: wide })`)).toEqual([]);
        expect(folds(`${wide}$saw('c3').outMono(0, wide)`)).toEqual([
            'wide‹.mono()›',
        ]);
        expect(folds(`${wide}$saw('c3').outMono({ gain: wide })`)).toEqual([
            'wide‹.mono()›',
        ]);
    });

    test("a stereo module's mono outputs are mono", () => {
        expect(folds(`$enosc('c3').outMono()`)).toEqual([
            "$enosc('c3')‹._output.mono()›",
        ]);
        expect(folds(`$enosc('c3')._odd.outMono()`)).toEqual([]);
        expect(folds(`const e = $enosc('c3')\ne._all.outMono()`)).toEqual([]);
    });

    test('an array folds element by element', () => {
        expect(folds(`${wide}$enosc([wide, 'c3']).out()`)).toEqual([
            'wide‹.mono()›',
        ]);
    });

    test('a value only a run can tell may be stereo', () => {
        expect(folds(`const d = $deferred()\n$enosc(d).out()`)).toEqual([
            'd‹.mono()›',
        ]);
        expect(folds(`const f = (x) => $enosc(x).out()`)).toEqual([
            'x‹.mono()›',
        ]);
        expect(folds(`$input.outMono()`)).toEqual(['$input‹.mono()›']);
        expect(folds(`$inputMono.outMono()`)).toEqual([]);
    });

    test('a collection of mono and stereo may be stereo', () => {
        expect(folds(`${wide}$c($saw('c3'), wide).outMono()`)).toEqual([
            "$c($saw('c3'), wide)‹.mono()›",
        ]);
        expect(folds(`$c($saw('c3'), $saw('e3')).outMono()`)).toEqual([]);
    });
});
