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
            hints(`const seq = $('c e g')\n$saw(seq).amp($perc(seq))`),
        ).toEqual(['$saw(seq‹cv›', 'mp($perc(seq‹trig›']);
    });

    test('a cycle resolves as argument, chain receiver and method receiver', () => {
        expect(
            hints(`const cyc = $('0*8')\ncyc.perc()\n$adsr(cyc)\ncyc.amp(2)`),
        ).toEqual(['cyc‹trig›', '$adsr(cyc‹gate›', 'cyc‹cv›']);
    });

    test('an inline chain puts the hint after the receiver', () => {
        const source = `$('0*8').perc()`;
        const [site] = collect(source);
        expect(source.slice(0, site.end)).toBe(`$('0*8')`);
        expect(site.output).toBe('trig');
    });

    test('a scope reads no output of its receiver', () => {
        expect(hints('$perc($`0`.scope())\n$`c e`.scope()')).toEqual([
            '$`0`.scope()‹trig›',
        ]);
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
                `const seqs = []\nfor (let i = 0; i < 5; i++) {\n    seqs.push($('c e g'))\n}\n$sine(seqs).amp($perc(seqs))`,
            ),
        ).toEqual(['$sine(seqs‹cv›', 'p($perc(seqs‹trig›']);
    });

    test('array literal elements resolve one by one', () => {
        expect(
            hints(`const a = $('c')\nconst b = $('e')\n$perc([a, b])`),
        ).toEqual(['$perc([a‹trig›', '$perc([a, b‹trig›']);
    });

    test('a config-object input resolves by its own type', () => {
        expect(
            hints(`const seq = $('c e')\n$saw('c', { restart: seq })`),
        ).toEqual(['restart: seq‹trig›']);
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
                `const seq = $('c e g')\nout: $saw(seq._cv).amp($perc(seq._trig))\n$sine('c').amp(2)\n$cycle($('c'))`,
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
        ).toEqual([
            "c').lpf('c')‹12db›",
            "'c').wet(seq‹cv›",
            'x) => x, seq‹cv›',
        ]);
    });

    test('a chain method option resolves by its own class', () => {
        expect(
            hints(
                `const seq = $('c e')\nout: $sine('c').erratic(1, { trigger: seq, speed: seq }).track({ gain: seq })`,
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

    test('$super resolves its signals as a call and as a chain', () => {
        expect(
            hints(
                `const seq = $('1 2')\n$super(seq, (f) => $saw(f), { width: seq })\nseq.super((f) => $saw(f), { detune: seq })`,
            ),
        ).toEqual([
            '$super(seq‹cv›',
            'f) => $saw(f‹output›',
            '{ width: seq‹cv›',
            'seq‹cv›',
            'f) => $saw(f‹output›',
            ' detune: seq‹cv›',
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
            hints(`const seq = $('c e')\n$saw(...[seq], { restart: seq })`),
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
        `const seq = $('c e g')\nout: $saw(seq).amp($perc(seq))`,
        `out: $saw($('c e g')).amp($perc($('c*4')))`,
        `const cyc = $('0*8')\nout: cyc.perc()\nout2: $adsr(cyc)\nout3: cyc.amp(2)`,
        `out: $('0*8').adsr()`,
        `const seqs = []\nfor (let i = 0; i < 3; i++) {\n    seqs.push($('c e g'))\n}\nout: $sine(seqs).amp($perc(seqs))`,
        `const a = $('c')\nconst b = $('e')\nout: $perc([a, b])`,
        `const seq = $('c e')\nout: $saw(seq, { restart: seq })`,
        `function voice(s) {\n    return $saw(s).amp($perc(s))\n}\nout: voice($('c e g'))`,
        `out: $perc($clock)\nout2: $tr808.kick($clock)`,
        `const seq = $('c e')\nout: seq\nout2: $sine('c').amp(seq)`,
        'out: $perc($`0`.scope())\nfunction f(s) {\n    return $adsr(s.scope())\n}\nf($`0 1`)',
        `out: $sine('c').lpf($('c5 c6'))`,
        `const seq = $('c e')\nout: $saw($g1([seq, seq]), { restart: $g2(seq) })`,
        `function f(s, m) {\n    return s.xover().wet(m)\n}\nf($clock, $('1 2'))`,
        `function f(m) {\n    return $sine('c').pipeMix((x) => x.lpf('c'), m)\n}\nout: f($('1 2'))`,
        `function f(s) {\n    return $sine('c').range(s, s)\n}\nout: f($('0 1'))`,
        `function f(s) {\n    return $sine('c').amp($g1([s, s])).track({ gain: $g1(s) })\n}\nout: f($('0 1'))`,
        `function f(s) {\n    return $mix([[s, 2.5]]).amp($step([s, 'c'], s))\n}\nout: f($('0 1'))`,
        `function f(s) {\n    return $quantizer($sine('c'), s)\n}\nout: f($('c, e, g'))`,
        `function f(s) {\n    return $sine('c').erratic(1, { trigger: s, speed: s })\n}\nout: f($('0 1')).track({ gain: $('1 2') })`,
        `function f(s) {\n    return $delay(s, s, { feedback: s, feedbackCb: (m) => m.lpf(s) })\n}\nf($('0 1'))`,
        `function f(s) {\n    return $sine('c').delay(s).wet(s)\n}\nf($('0 1'))`,
        `const seq = $('[c,e] [g,b]')\nout: $perc(seq.slice(1))\nout2: $saw(seq.chunk(1)).amp($perc(seq[1]))\nout3: seq.slice(1).perc()`,
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
        expect(folds(`${wide}out: $enosc(wide)`)).toEqual(['wide‹.mono()›']);
        expect(
            folds(`${wide}out: $plate($saw('c3'), { decay: wide })`),
        ).toEqual(['wide‹.mono()›']);
        expect(folds(`${wide}out: wide.enosc()`)).toEqual(['wide‹.mono()›']);
        expect(
            folds(
                `${wide}out: $sampler('kick.wav', 5, { slice: [[0, 0.5], wide] })`,
            ),
        ).toEqual(['wide‹.mono()›']);
    });

    test('a mono out folds what it sends', () => {
        expect(folds(`out$0m: $stereo(0, 1)`)).toEqual([
            '$stereo(0, 1)‹.mono()›',
        ]);
        expect(folds(`${wide}out$0m: wide.lpf('c5')`)).toEqual([
            "wide.lpf('c5')‹._12db.mono()›",
        ]);
    });

    test('a module root folds through the output it reads', () => {
        expect(folds(`out$0m: $coinFlip($stereo('c', 'g'))`)).toEqual([
            "$coinFlip($stereo('c', 'g'))‹._heads.mono()›",
        ]);
    });

    test('an input read side by side does not fold', () => {
        expect(folds(`${wide}out: $pan(wide, 2)`)).toEqual([]);
        expect(folds(`${wide}out$0m: $mono(wide)`)).toEqual([]);
        expect(folds(`${wide}out$0m: wide.mono()`)).toEqual([]);
    });

    test('a lifted module carries what its inputs carry', () => {
        expect(folds(`${wide}out: $enosc(wide.lpf('c5').add(1))`)).toEqual([
            "wide.lpf('c5').add(1)‹.mono()›",
        ]);
        expect(folds(`out$0m: $saw('c3').lpf('c5')`)).toEqual([]);
        expect(folds(`out: $enosc($saw($sine('1hz')))`)).toEqual([]);
    });

    test('a signal group carries the channels of its signals', () => {
        expect(folds('out$0m: $`0`.add($g1(0))')).toEqual([]);
        expect(folds(`out$0m: $saw($g1(['c3', 'e3']))`)).toEqual([]);
        expect(folds(`${wide}out$0m: $saw('c3').add($g1(wide))`)).toEqual([
            "$saw('c3').add($g1(wide))‹.mono()›",
        ]);
        expect(
            folds('const $g1 = () => $stereo(0, 1)\nout$0m: $`0`.add($g1(0))'),
        ).toEqual(['$`0`.add($g1(0))‹.mono()›']);
    });

    test('a lane or slice carries its collection’s channels', () => {
        expect(folds(`out$0m: $saw('c3').lane(0)`)).toEqual([]);
        expect(folds(`out$0m: $saw(['c3', 'e3']).slice(0, 1)`)).toEqual([]);
        expect(folds(`${wide}out$0m: wide.lane(0)`)).toEqual([
            'wide.lane(0)‹.mono()›',
        ]);
    });

    test('a pattern’s cycle is mono', () => {
        expect(folds(`out$0m: $('c3 e3').cycle()`)).toEqual([]);
        expect(folds('out$0m: $saw($`c3 e3`.cycle())')).toEqual([]);
    });

    test('a crossfade lifts with its playhead', () => {
        expect(folds(`${wide}out$0m: $cross(3, wide.range(0, 1))`)).toEqual([
            '$cross(3, wide.range(0, 1))‹.mono()›',
        ]);
        expect(
            folds(`out$0m: $cross(3, $sine('1hz').range(0, 1), [0, 5])`),
        ).toEqual([]);
    });

    test('a track tap’s controls are mono', () => {
        const tap = `bass2: $saw('c2')\nconst bass = $trackTap('bass')\n`;
        expect(
            folds(`${tap}out: $sine('c5').track({ pan: bass.panning })`),
        ).toEqual([]);
        expect(folds(`${tap}out$0m: bass.level`)).toEqual([]);
        expect(folds(`${tap}out$0m: bass['level']`)).toEqual([]);
        expect(folds(`${tap}const { active } = bass\nout$0m: active`)).toEqual(
            [],
        );
        expect(folds(`${tap}out$0m: bass`)).toEqual(['bass‹.mono()›']);
    });

    test('either side of a choice may be the value', () => {
        expect(folds(`${wide}const x = 0 || wide\nout$0m: x`)).toEqual([
            'x‹.mono()›',
        ]);
        expect(folds(`const x = 0 || $saw('c3')\nout$0m: x`)).toEqual([]);
        const coin = `const coin = Math.random() > 0.5\n`;
        expect(
            folds(
                `${coin}const x = coin ? $saw('c3') : $sine('c3')\nout$0m: x`,
            ),
        ).toEqual([]);
        expect(
            folds(
                `${wide}${coin}const x = coin ? $saw('c3') : wide\nout$0m: x`,
            ),
        ).toEqual(['x‹.mono()›']);
        expect(folds(`const x = undefined ?? $saw('c3')\nout$0m: x`)).toEqual(
            [],
        );
    });

    test('a destructured const carries what it takes', () => {
        expect(folds(`const { _cv: pitch } = $('c3')\nout$0m: pitch`)).toEqual(
            [],
        );
        expect(folds(`const { _cv } = $('c3')\nout$0m: _cv`)).toEqual([]);
        expect(
            folds(
                `const [a, b] = [$saw('c3'), $stereo(0, 1)]\nout$0m: a\nout2$0m: b`,
            ),
        ).toEqual(['b‹.mono()›']);
        expect(
            folds(`${wide}const [a = wide] = [$saw('c3')]\nout$0m: a`),
        ).toEqual(['a‹.mono()›']);
    });

    test('plain values, patterns and controls are mono', () => {
        expect(folds(`out: $enosc('c3')`)).toEqual([]);
        expect(folds(`out: $enosc($('c3 e3'))`)).toEqual([]);
        expect(folds(`const n = 3\nout: $enosc(n)`)).toEqual([]);
        expect(folds(`out$0m: $saw($clock._tempo)`)).toEqual([]);
    });

    test('a buffer reader carries what its buffer records', () => {
        expect(
            folds(`${wide}out: $enosc($delayRead($buffer(wide, 1), 0.1))`),
        ).toEqual(['$delayRead($buffer(wide, 1), 0.1)‹.mono()›']);
        expect(
            folds(
                `const b = $buffer($saw('c3'), 1)\nout: $enosc($delayRead(b, 0.1))`,
            ),
        ).toEqual([]);
    });

    test('patterns and missing values are mono', () => {
        expect(folds(`out: $enosc($('c3 e3').fast(2).slow(3))`)).toEqual([]);
        expect(folds(`out$0m: $saw('c3').track({ gain: undefined })`)).toEqual(
            [],
        );
    });

    test('a range lifts with its bounds', () => {
        expect(
            folds(`out: $enosc($sine('1hz').range($stereo(0, 1), 5))`),
        ).toEqual(["$sine('1hz').range($stereo(0, 1), 5)‹.mono()›"]);
        expect(folds(`out: $enosc($sine('1hz').range(0, 5))`)).toEqual([]);
    });

    test("an out's folding options", () => {
        expect(folds(`${wide}out: $saw('c3').track({ pan: wide })`)).toEqual([
            'wide‹.mono()›',
        ]);
        expect(folds(`${wide}out: $saw('c3').track({ gain: wide })`)).toEqual(
            [],
        );
        expect(
            folds(`${wide}out$0m: $saw('c3').track({ gain: wide })`),
        ).toEqual(['wide‹.mono()›']);
        expect(
            folds(`${wide}out$0m: $saw('c3').track({ gain: wide })`),
        ).toEqual(['wide‹.mono()›']);
    });

    test('a .track() a mono label plays through a const folds', () => {
        expect(
            folds(
                `${wide}const t = $saw('c3').track({ gain: wide })\nout$0m: t`,
            ),
        ).toEqual(['wide‹.mono()›']);
        expect(
            folds(`${wide}out$0m: ($saw('c3').track({ gain: wide }))`),
        ).toEqual(['wide‹.mono()›']);
    });

    test("a stereo module's mono outputs are mono", () => {
        expect(folds(`out$0m: $enosc('c3')`)).toEqual([
            "$enosc('c3')‹._all.mono()›",
        ]);
        expect(folds(`out$0m: $enosc('c3')._low`)).toEqual([]);
        expect(folds(`const e = $enosc('c3')\nout$0m: e._root`)).toEqual([]);
        expect(folds(`out$0m: $enosc('c3').concat($enosc('g3'))._low`)).toEqual(
            [],
        );
        expect(folds(`out$0m: $enosc('c3').scope()._high`)).toEqual([]);
    });

    test('a cycle of const bindings reads as unknown', () => {
        expect(folds(`const a = b\nconst b = a\nout$0m: a._low`)).toEqual([
            'a._low‹.mono()›',
        ]);
    });

    test('an array folds element by element', () => {
        expect(folds(`${wide}out: $enosc([wide, 'c3'])`)).toEqual([
            'wide‹.mono()›',
        ]);
    });

    test('a value only a run can tell may be stereo', () => {
        expect(folds(`const d = $deferred()\nout: $enosc(d)`)).toEqual([
            'd‹.mono()›',
        ]);
        expect(folds(`const f = (x) => $enosc(x).scope()`)).toEqual([
            'x‹.mono()›',
        ]);
        expect(folds(`out$0m: $input`)).toEqual(['$input‹.mono()›']);
        expect(folds(`out$0m: $sampler('kick.wav', 5)`)).toEqual([
            "$sampler('kick.wav', 5)‹.mono()›",
        ]);
        expect(folds(`out$0m: $inputMono`)).toEqual([]);
    });

    test('a collection of mono and stereo may be stereo', () => {
        expect(folds(`${wide}out$0m: $c($saw('c3'), wide)`)).toEqual([
            "$c($saw('c3'), wide)‹.mono()›",
        ]);
        expect(folds(`out$0m: $c($saw('c3'), $saw('e3'))`)).toEqual([]);
    });
});
