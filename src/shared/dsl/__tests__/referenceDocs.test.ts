import { describe, expect, test } from 'vitest';

import schemas from '@modular/core/schemas.json';
import { buildLibSource } from '../../../main/dsl/typescriptLibGen';
import { formatDsl } from '../../../renderer/components/monaco/formattingProvider';
import { GLOBAL_ENTRIES } from '../globalDocs';
import type { DocContent } from '../referenceDocs';
import {
    moduleToReferenceEntry,
    renderDocContentJsDoc,
    tableToCompactMarkdown,
} from '../referenceDocs';

const DOCS: DocContent = {
    categories: ['pitch'],
    description: 'Outputs every note of a chord.',
    examples: [
        { code: "out: $saw($chord('C3[maj7]'))", title: 'Pad' },
        { code: "out: $sine($chord('A[m9]'))", title: 'Bells' },
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
    const lines = renderDocContentJsDoc(DOCS, '$chord');

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
            "out: $saw($chord('C3[maj7]'))",
            '```',
            '',
            'More in the docs: {@link $chord}',
        ]);
    });

    test('no reference link when nothing is left out', () => {
        const one = { ...DOCS, examples: DOCS.examples.slice(0, 1) };
        expect(renderDocContentJsDoc(one, '$chord').at(-1)).toBe('```');
    });
});

const ENTRIES = [...schemas.map(moduleToReferenceEntry), ...GLOBAL_ENTRIES];

/** Internal modules and `$buffer` have no factory in the lib. */
const inLib = (name: string) => !name.startsWith('_') && name !== '$buffer';

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
        const examples = [
            ...docs.examples,
            ...docs.sections.flatMap((s) => s.examples),
        ];
        for (const { code, title } of examples) {
            if ((await formatDsl(code)) !== code) {
                unformatted.push(`${name} "${title}"`);
            }
        }
    }
    expect(unformatted).toEqual([]);
});
