import { describe, expect, test } from 'vitest';

import referenceCategories from '@modular/core/referenceCategories.json';
import schemas from '@modular/core/schemas.json';
import { buildLibSource } from '../../../main/dsl/typescriptLibGen';
import { formatDsl } from '../../../renderer/components/monaco/formattingProvider';
import { GLOBAL_ENTRIES } from '../globalDocs';
import type { DocContent } from '../referenceDocs';
import {
    allExamples,
    moduleToReferenceEntry,
    renderDocContentJsDoc,
    tableToCompactMarkdown,
} from '../referenceDocs';

const DOCS: DocContent = {
    categories: ['pitch'],
    description: 'Outputs every note of a chord.',
    examples: [
        { code: "out: $saw('c3:M7')", title: 'Pad' },
        { code: "out: $sine('a:m9')", title: 'Bells' },
    ],
    sections: [
        {
            body: 'A root, then a chord.',
            examples: [],
            table: {
                columns: ['Form', 'Example', 'Gives'],
                rows: [['`root[chord]`', "`'C[maj7]'`", 'the chord']],
            },
            title: 'Syntax',
        },
    ],
    summary: 'Notes of a chord',
};

describe('tableToCompactMarkdown', () => {
    test('one bullet per row, later columns labelled', () => {
        expect(tableToCompactMarkdown(DOCS.sections[0].table!)).toEqual([
            "- `root[chord]` — `'C[maj7]'` · *Gives:* the chord",
        ]);
    });
});

describe('renderDocContentJsDoc', () => {
    const lines = renderDocContentJsDoc(DOCS, '$saw');

    test('description, sections, then the first example', () => {
        expect(lines).toEqual([
            'Outputs every note of a chord.',
            '',
            '**Syntax**',
            '',
            'A root, then a chord.',
            '',
            "- `root[chord]` — `'C[maj7]'` · *Gives:* the chord",
            '',
            '```js',
            '// Pad',
            "out: $saw('c3:M7')",
            '```',
            '',
            'More in the docs: {@link $saw}',
        ]);
    });

    test('no reference link when nothing is left out', () => {
        const one = { ...DOCS, examples: DOCS.examples.slice(0, 1) };
        expect(renderDocContentJsDoc(one, '$saw').at(-1)).toBe('```');
    });
});

const ENTRIES = [...schemas.map(moduleToReferenceEntry), ...GLOBAL_ENTRIES];

/** Internal modules and `$buffer` have no factory in the lib. */
const inLib = (name: string) => !name.startsWith('_') && name !== '$buffer';

const CATEGORY_NAMES = new Set(referenceCategories.map((c) => c.name));

/** Markdown the editor renders as blocks, which only sections' tables and
 *  examples may carry. */
const BLOCK_MARKDOWN: [string, RegExp][] = [
    ['a code fence', /```/],
    ['a heading', /^\s*#/m],
    ['a table row', /^\s*\|/m],
    ['HTML', /^\s*</m],
    ['a JSDoc link', /\{@link/],
];

test('every entry has well-formed docs', () => {
    const problems: string[] = [];
    for (const { name, docs } of ENTRIES) {
        const fail = (what: string) => problems.push(`${name}: ${what}`);
        const words = docs.summary.split(/\s+/).filter(Boolean).length;
        if (docs.summary.includes('\n') || words < 1 || words > 6) {
            fail(`summary must be one line of 1–6 words: ${docs.summary}`);
        }
        if (docs.summary.endsWith('.')) {
            fail('summary ends with a period');
        }
        if (!docs.description.trim()) {
            fail('empty description');
        }
        const prose = [
            ['description', docs.description],
            ...docs.sections.map((s) => [`section "${s.title}"`, s.body]),
        ];
        for (const [where, text] of prose) {
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
        for (const { title, code } of allExamples(docs)) {
            if (!title.trim() || !code.trim()) {
                fail('an example needs a title and code');
            }
            if (titles.has(title)) {
                fail(`duplicate example title ${title}`);
            }
            titles.add(title);
        }
        for (const { title, body, table, examples } of docs.sections) {
            if (!title.trim()) {
                fail('a section needs a title');
            }
            if (!body.trim() && !table && examples.length === 0) {
                fail(`section "${title}" is empty`);
            }
            if (table) {
                if (table.columns.length < 2 || table.rows.length === 0) {
                    fail(`table in "${title}" needs 2+ columns and a row`);
                }
                const ragged = table.rows.some(
                    (row) =>
                        row.length !== table.columns.length ||
                        row.some((cell) => !cell.trim()),
                );
                if (ragged) {
                    fail(`table in "${title}" has a ragged or empty row`);
                }
            }
        }
    }
    expect(problems).toEqual([]);
});

describe('docs in the generated lib and the reference', () => {
    const lib = buildLibSource(schemas);

    test('every entry carries its description and first example', () => {
        for (const { docs, name } of ENTRIES.filter((e) => inLib(e.name))) {
            const firstLine = docs.description.split('\n')[0];
            expect(lib, name).toContain(firstLine);
            const first = docs.examples[0];
            if (first) {
                expect(lib, name).toContain(`// ${first.title}`);
            }
        }
    });

    test('a module entry carries its params, outputs and docs', () => {
        const lpf = schemas.find((m) => m.name === '$lpf')!;
        const entry = moduleToReferenceEntry(lpf);
        expect(entry.signature.startsWith('$lpf(input, cutoff')).toBe(true);
        expect(entry.params.find((p) => p.name === 'resonance')?.meta).toBe(
            '≥ 0, default 0',
        );
        expect(entry.outputs?.find((o) => o.main)).toBeDefined();
        expect(entry.docs).toBe(lpf.docs);
    });
});

test('every example is formatted the way Format Document prints it', async () => {
    const unformatted: string[] = [];
    for (const { docs, name } of ENTRIES) {
        for (const { code, title } of allExamples(docs)) {
            if ((await formatDsl(code)) !== code) {
                unformatted.push(`${name} "${title}"`);
            }
        }
    }
    expect(unformatted).toEqual([]);
});
