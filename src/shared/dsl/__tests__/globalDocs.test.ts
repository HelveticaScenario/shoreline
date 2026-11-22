import { describe, expect, test } from 'vitest';

import schemas from '@modular/core/schemas.json';
import { buildLibSource } from '../../../main/dsl/typescriptLibGen';
import { VIDEO_GLOBALS } from '../../../main/dsl/videoLibGen';
import { GLOBAL_ENTRIES, GLOBALS } from '../globalDocs';
import { parseDeclaration } from '../tsDeclaration';

const entry = (name: string) => GLOBAL_ENTRIES.find((e) => e.name === name)!;

test('every global documents exactly its first declaration’s params', () => {
    for (const global of GLOBALS) {
        const [first, ...overloads] = global.declarations.map(parseDeclaration);
        expect(Object.keys(global.params).sort(), first.name).toEqual(
            first.params.map((p) => p.name).sort(),
        );
        for (const overload of overloads) {
            expect(overload.name, first.name).toBe(first.name);
        }
    }
});

describe('the generated lib', () => {
    const lib = buildLibSource(schemas);

    test('declares every global as its docs do', () => {
        for (const global of GLOBALS) {
            for (const declaration of global.declarations) {
                expect(lib).toContain(`function ${declaration};`);
            }
        }
    });

    test('declares no global function without docs', () => {
        const documented = new Set([
            ...GLOBAL_ENTRIES.map((e) => e.name),
            ...VIDEO_GLOBALS.map((name) => `$${name}`),
        ]);
        const declared = [
            ...lib.matchAll(/^(?:declare )?function (\$[\w$]*)[(<]/gm),
        ].map((m) => m[1]);
        expect(declared.filter((name) => !documented.has(name))).toEqual([]);
    });
});

describe('reference entries', () => {
    test('a param is typed as every overload types it', () => {
        expect(entry('$slider').params.find((p) => p.name === 'value')).toEqual(
            {
                description: 'The starting value, a literal',
                name: 'value',
                type: 'number | HZ | Note',
            },
        );
        expect(entry('$hz').returns?.type).toBe(
            'number | number[] | Collection',
        );
    });

    test('the signature names params as a call reads them', () => {
        expect(entry('$slider').signature).toBe(
            '$slider(label, value, min, max, group?)',
        );
        expect(entry('$c').signature).toBe('$c(...args)');
        expect(entry('$c').params[0]).toMatchObject({
            name: 'args',
            rest: true,
        });
        expect(entry('$bus').signature).toBe('$bus()');
    });

    test('a type param shows as its constraint', () => {
        expect(entry('$cartesian').params[0].type).toBe('unknown[][]');
    });

    test('a function-typed param keeps its arrow', () => {
        expect(entry('$setEndOfChainCb').params[0].type).toBe(
            '(mixed: Collection) => Collection | CollectionWithRange',
        );
    });

    test('members keep their dot', () => {
        expect(entry('$trackTap').returns?.members?.map((m) => m.name)).toEqual(
            ['.active', '.level', '.panning'],
        );
    });
});
