import { describe, expect, test } from 'vitest';

import moduleCategories from '@modular/core/moduleCategories.json';
import schemas from '@modular/core/schemas.json';
import { buildLibSource } from '../../../main/dsl/typescriptLibGen';
import { GLOBAL_ENTRIES, GLOBALS } from '../globalDocs';
import { parseDeclaration } from '../tsDeclaration';

const CATEGORY_NAMES = new Set(moduleCategories.map((c) => c.name));

/** Markdown a one-paragraph description cannot hold. */
const BLOCK_MARKDOWN: [string, RegExp][] = [
    ['a code fence', /```/],
    ['a heading', /(^|\n)\s*#{1,6}\s/],
    ['a list', /(^|\n)\s*([-*]|\d+\.)\s/],
    ['a table', /(^|\n)\s*\|/],
    ['a JSDoc link', /\{@link/],
];

const entry = (name: string) => GLOBAL_ENTRIES.find((e) => e.name === name)!;

test('every global has well-formed docs', () => {
    const problems: string[] = [];
    for (const { name, docs } of GLOBAL_ENTRIES) {
        const fail = (what: string) => problems.push(`${name}: ${what}`);
        const words = docs.summary.split(/\s+/).filter(Boolean).length;
        if (docs.summary.includes('\n') || words < 1 || words > 6) {
            fail(`summary must be one line of 1–6 words: ${docs.summary}`);
        }
        if (docs.summary.endsWith('.')) {
            fail('summary ends with a period');
        }
        const texts = [
            ['description', docs.description],
            ...docs.sections.map((s) => [`section "${s.title}"`, s.body]),
        ];
        if (!docs.description.trim()) {
            fail('empty description');
        }
        for (const [where, text] of texts) {
            for (const [what, re] of BLOCK_MARKDOWN) {
                if (re.test(text)) {
                    fail(`${where} contains ${what}`);
                }
            }
        }
        if (docs.categories.length === 0) {
            fail('no categories');
        }
        if (new Set(docs.categories).size !== docs.categories.length) {
            fail('duplicate categories');
        }
        for (const category of docs.categories) {
            if (!CATEGORY_NAMES.has(category)) {
                fail(`unknown category ${category}`);
            }
        }
        const titles = new Set<string>();
        const examples = [
            ...docs.examples,
            ...docs.sections.flatMap((s) => s.examples),
        ];
        if (examples.length === 0) {
            fail('no examples');
        }
        for (const { title, code } of examples) {
            if (!title.trim() || !code.trim()) {
                fail('an example needs a title and code');
            }
            if (titles.has(title)) {
                fail(`duplicate example title ${title}`);
            }
            titles.add(title);
        }
        for (const section of docs.sections) {
            if (!section.title.trim()) {
                fail('a section needs a title');
            }
            if (
                !section.body.trim() &&
                !section.table &&
                section.examples.length === 0
            ) {
                fail(`section "${section.title}" is empty`);
            }
        }
    }
    expect(problems).toEqual([]);
});

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
        const documented = new Set(GLOBAL_ENTRIES.map((e) => e.name));
        // `$scopeXY` is documented with the video namespace.
        documented.add('$scopeXY');
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
