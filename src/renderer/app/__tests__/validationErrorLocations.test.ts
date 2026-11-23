import { describe, expect, test } from 'vitest';

import { transformErrorsWithSourceLocations } from '../validationErrorLocations';
import type { ValidationError } from '@modular/core';
import type { SourceLocationInfo } from '../../../shared/ipcTypes';

const err = (location: string, moduleId?: string): ValidationError => ({
    field: 'params.freq',
    location,
    message: 'invalid value',
    moduleId,
});

const loc = (line: number, idIsExplicit = false): SourceLocationInfo => ({
    column: 1,
    idIsExplicit,
    line,
});

describe('transformErrorsWithSourceLocations', () => {
    test("an error takes its own module's line, even among several of its type", () => {
        const map = { '$sine-1': loc(3), '$sine-2': loc(9) };
        const [out] = transformErrorsWithSourceLocations(
            [err('$sine(...)', '$sine-2')],
            map,
        );
        expect(out.location).toBe('line 9');
    });

    test('an explicit id keeps its name beside the line', () => {
        const map = { myOsc: loc(4, true) };
        const [out] = transformErrorsWithSourceLocations(
            [err("'myOsc'", 'myOsc')],
            map,
        );
        expect(out.location).toBe("'myOsc' line 4");
    });

    test('an error without a located module is left as it is', () => {
        const map = { '$sine-1': loc(3) };
        const input = [err('$sine(...)'), err('$saw(...)', '$saw-1')];
        expect(transformErrorsWithSourceLocations(input, map)).toEqual(input);
        expect(transformErrorsWithSourceLocations(input)).toEqual(input);
    });
});
