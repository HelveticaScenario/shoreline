import { describe, expect, test } from 'vitest';
import schemas from '@modular/core/schemas.json';
import type { ModuleSchema } from '@modular/core';

import { analyzeSourceSpans } from '../analyzeSource';

function analyze(source: string) {
    return analyzeSourceSpans(source, schemas as ModuleSchema[]);
}

/** Registry entries as `key moduleType arg=text …`, spans read back from
 *  the source. */
function entries(source: string): string[] {
    return [...analyze(source).registry].map(
        ([key, { moduleType, args }]) =>
            `${key} ${moduleType} ${[...args]
                .map(([name, s]) => `${name}=${source.slice(s.start, s.end)}`)
                .join(' ')}`,
    );
}

describe('argument span registry', () => {
    test('is keyed by the patch line and column a factory captures', () => {
        expect(entries("const a = 1\n  $sine('c4').out()")).toEqual([
            "2:3 $sine freq='c4'",
        ]);
    });

    test('config keys are read by name, quoted or not', () => {
        expect(entries("$sine('c4', { 'phase': 0.25, [k]: 1 }).out()")).toEqual(
            ["1:5 $sine freq='c4' phase=0.25"],
        );
    });

    test('a signed number is a literal; any other prefix expression is not', () => {
        expect(
            entries(
                '$sine(-1).out()\n$sine(+2).out()\n$sine(-x).out()\n$sine(!1).out()',
            ),
        ).toEqual(['1:5 $sine freq=-1', '2:1 $sine freq=+2']);
    });

    test('a dotted callee resolves to the module it names', () => {
        const [kick808, kick909] = entries(
            "$tr808.kick('c1').out()\n$tr909.kick('c1').out()",
        );
        expect(kick808).toMatch(/^1:\d+ \$tr808\.kick /);
        expect(kick909).toMatch(/^2:\d+ \$tr909\.kick /);
    });

    test('array elements are recorded up to the first spread', () => {
        expect(entries("$sine(['c4', , 'e4', ...more, 'g4']).out()")).toEqual([
            "1:5 $sine freq.0='c4' freq.2='e4'",
        ]);
    });

    test('only a bare $p call is the pattern builder', () => {
        expect(entries("$p('a b')\nobj.$p('c d')")).toEqual([
            "1:5 $p source='a b'",
        ]);
    });

    test('pattern methods are the ones the analyzer knows, not inherited names', () => {
        expect(entries("$p('a b').fast(2).constructor(5).toString(3)")).toEqual(
            ['1:15 .fast factor=2', "1:5 $p source='a b'"],
        );
    });
});

/** Each resolution of the first interpolated template as
 *  `name@evaluatedStart+evaluatedLength`. */
function resolutions(source: string): string[] {
    const [list] = [...analyze(source).interpolationResolutions.values()];
    return (list ?? []).map(
        (r) =>
            `${source.slice(r.constLiteralSpan.start, r.constLiteralSpan.end)}@${r.evaluatedStart}+${r.evaluatedLength}`,
    );
}

describe('template interpolation', () => {
    test('a number written in place counts as the text it prints', () => {
        expect(
            resolutions("const root = 'c4'\n$p(`${0x10} ${root} e4`)"),
        ).toEqual(["'c4'@3+2"]);
    });

    test('a const holding a number counts toward later offsets, unredirected', () => {
        expect(
            resolutions(
                "const n = 0x10\nconst m = -2\nconst root = 'c4'\n$p(`${n} ${m} ${root}`)",
            ),
        ).toEqual(["'c4'@6+2"]);
    });

    test('a const template is redirected into only when all of it redirects', () => {
        expect(
            resolutions(
                "const n = 4\nconst pat = `c${n} e4`\nconst root = 'g4'\n$p(`${pat} ${root}`)",
            ),
        ).toEqual(["'g4'@6+2"]);
        expect(
            resolutions(
                "const root = 'c4'\nconst pat = `${root} e4`\n$p(`${pat} g4`)",
            ),
        ).toEqual(['`${root} e4`@0+5']);
    });

    test('a const inside a function resolves where it is in scope', () => {
        expect(
            resolutions(
                "function voice() {\n  const inner = 'e4'\n  return $p(`${inner} c4`)\n}",
            ),
        ).toEqual(["'e4'@0+2"]);
    });

    test('a reference resolves through the nearest binding of its name', () => {
        // A helper's own const of the same name hides nothing outside it.
        expect(
            entries(
                "const notes = 'c4 e4'\nfunction f() { const notes = 'g4'; return notes }\n$p(notes)",
            ),
        ).toEqual(["3:1 $p source='c4 e4'"]);
        // A `let` is no const, whatever a function elsewhere declares.
        expect(
            entries(
                "let pat = 'x y'\nfunction v() { const pat = 'c4 e4' }\n$p(pat)",
            ),
        ).toEqual([]);
        // A loop variable hides the outer const of its name.
        expect(
            entries(
                "const n = 'c4'\nfor (const n of ['d4']) { $sine(n).out() }",
            ),
        ).toEqual([]);
        // A `var` binds in its whole function, from inside any block.
        expect(
            entries(
                "const x = 'c4'\nfunction f(c) { if (c) { var x = 'd4' } $sine(x).out() }",
            ),
        ).toEqual([]);
        // A class static block holds its own `var`.
        expect(
            entries(
                "const x = 'c4'\nclass K { static { if (1) { var x = 'd4' } $sine(x).out() } }",
            ),
        ).toEqual([]);
        expect(
            entries(
                "const x = 'c4'\nclass K { static { var x = 'd4' } }\n$sine(x).out()",
            ),
        ).toEqual(["3:1 $sine freq='c4'"]);
        // A named class expression binds its own name inside itself.
        expect(
            entries(
                "const x = 'c4'\nconst C = class x { m() { $sine(x).out() } }",
            ),
        ).toEqual([]);
    });
});
