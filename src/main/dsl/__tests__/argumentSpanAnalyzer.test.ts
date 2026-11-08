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
});
