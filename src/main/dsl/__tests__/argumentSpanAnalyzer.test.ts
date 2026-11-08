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
});
