import { describe, expect, test } from 'vitest';

import schemas from '@modular/core/schemas.json';
import { buildLibSource } from '../../../main/dsl/typescriptLibGen';
import { VIDEO_GLOBALS } from '../../../main/dsl/videoLibGen';
import { dslModules } from '../dslModuleDocs';
import { GLOBAL_ENTRIES, GLOBALS } from '../globalDocs';
import { functionToReferenceEntry } from '../referenceDocs';
import { chainDeclaration, parseDeclaration } from '../tsDeclaration';

const entry = (name: string) => GLOBAL_ENTRIES.find((e) => e.name === name)!;

/** Every documented function: the globals and the modules the DSL builds. */
const FUNCTIONS = [...GLOBALS, ...dslModules(schemas)];

test('every function documents exactly its first declaration’s params', () => {
    for (const fn of FUNCTIONS) {
        const [first, ...overloads] = fn.declarations.map(parseDeclaration);
        expect(Object.keys(fn.params).sort(), first.name).toEqual(
            first.params.map((p) => p.name).sort(),
        );
        for (const overload of overloads) {
            expect(overload.name, first.name).toBe(first.name);
        }
    }
});

describe('the generated lib', () => {
    const lib = buildLibSource(schemas);

    test('declares every function as its docs do', () => {
        for (const fn of FUNCTIONS) {
            for (const declaration of fn.declarations) {
                expect(lib).toContain(`function ${declaration};`);
                if (fn.chainable) {
                    expect(lib).toContain(
                        `  ${chainDeclaration(declaration, 'Plain')};`,
                    );
                }
            }
        }
    });

    test('declares no function without docs', () => {
        const documented = new Set([
            ...FUNCTIONS.map((f) => functionToReferenceEntry(f).name),
            ...VIDEO_GLOBALS.map((name) => `$${name}`),
        ]);
        const declared = [
            ...lib.matchAll(/^(?:declare )?function (\$[\w$]*)[(<]/gm),
        ].map((m) => m[1]);
        expect(declared.filter((name) => !documented.has(name))).toEqual([]);
    });
});

test('a chain form drops the receiver and keeps the rest as written', () => {
    expect(
        chainDeclaration(
            '$echo<T extends Signal>(input: Collection, time?: T, ...taps: number[]): Collection',
            'Plain',
        ),
    ).toBe('echo<T extends Signal>(time?: T, ...taps: number[]): Plain');
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
