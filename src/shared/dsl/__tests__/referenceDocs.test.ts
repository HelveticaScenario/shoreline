import { describe, expect, test } from 'vitest';

import schemas from '@modular/core/schemas.json';
import { buildLibSource } from '../../../main/dsl/typescriptLibGen';
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
        { code: "$saw($chord('C3[maj7]')).out()", title: 'Pad' },
        { code: "$sine($chord('A[m9]')).out()", title: 'Bells' },
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
            "$saw($chord('C3[maj7]')).out()",
            '```',
            '',
            'More in the reference: {@link $chord}',
        ]);
    });

    test('no reference link when nothing is left out', () => {
        const one = { ...DOCS, examples: DOCS.examples.slice(0, 1) };
        expect(renderDocContentJsDoc(one, '$chord').at(-1)).toBe('```');
    });
});

describe('module docs in the generated lib and the reference', () => {
    const lib = buildLibSource(schemas);

    test('every public module carries its description and first example', () => {
        for (const module of schemas) {
            if (module.name.startsWith('_') || module.name === '$buffer') {
                continue;
            }
            const firstLine = module.docs.description.split('\n')[0];
            expect(lib, module.name).toContain(firstLine);
            const first = module.docs.examples[0];
            if (first) {
                expect(lib, module.name).toContain(`// ${first.title}`);
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
