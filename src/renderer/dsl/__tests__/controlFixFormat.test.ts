import { describe, expect, test } from 'vitest';

import * as prettier from 'prettier/standalone';
import * as prettierBabel from 'prettier/plugins/babel';
import * as prettierEstree from 'prettier/plugins/estree';

import { formatControlQuickFix } from '../controlFixFormat';
import { addFix, apply, fixesAt } from './controlQuickFixHarness';

describe('formatControlQuickFix — layout', () => {
    const PRETTIER = {
        bracketSpacing: true,
        printWidth: 80,
        semi: true,
        singleQuote: true,
        tabWidth: 2,
        trailingComma: 'all' as const,
        useTabs: false,
    };

    async function formattedAdd(
        text: string,
        param: string,
        options: Record<string, unknown> = {},
    ) {
        const { source, fixes } = fixesAt(text);
        const fix = await formatControlQuickFix(source, addFix(fixes, param), {
            ...PRETTIER,
            ...options,
        });
        return { fix, out: apply(source, fix), source };
    }

    /** What Format Document prints for the same patch. */
    async function documentFormat(
        text: string,
        options: Record<string, unknown> = {},
    ) {
        return (
            await prettier.format(text, {
                ...PRETTIER,
                ...options,
                endOfLine: text.includes('\r\n') ? 'crlf' : 'lf',
                parser: 'babel',
                plugins: [prettierBabel, prettierEstree as never],
            })
        ).trimEnd();
    }

    test('the edited statement matches Format Document', async () => {
        const { out } = await formattedAdd(
            `$saw('a3', {\n  shape: 2,\n  sync: 4|\n}).out();`,
            'fm',
        );
        expect(out).toBe(await documentFormat(out));
        expect(out).toContain(`  fm: $slider('fm', 0, -5, 5),\n`);
    });

    test('the caret lands just past the inserted control', async () => {
        const { out, fix } = await formattedAdd(
            `$saw('a3', { shape: 2.5| }).out();`,
            'phaseOffset',
            { printWidth: 40 },
        );
        const at = fix.span.start + fix.caretOffset;
        expect(
            out.slice(out.lastIndexOf('$slider(', at), at).replace(/\s/g, ''),
        ).toBe(`$slider('phaseOffset',0,0,1,)`);
    });

    test('a nested statement keeps its indentation', async () => {
        const { out } = await formattedAdd(
            `function f() {\n    $saw('a3', {\n        shape: 2,|\n    }).out();\n}`,
            'fm',
            { tabWidth: 4 },
        );
        expect(out).toBe(
            `function f() {\n    $saw('a3', {\n        shape: 2,\n        fm: $slider('fm', 0, -5, 5),\n    }).out();\n}`,
        );
    });

    test('tabs and trailingComma none follow the options', async () => {
        const { out } = await formattedAdd(
            `function f() {\n\tconst s = $saw('a3', { shape: 2.5| });\n}`,
            'phaseOffset',
            { printWidth: 40, trailingComma: 'none', useTabs: true },
        );
        const options = {
            printWidth: 40,
            trailingComma: 'none',
            useTabs: true,
        };
        expect(out).toBe(await documentFormat(out, options));
        expect(out).toContain(`\t\tphaseOffset: $slider(\n`);
        expect(out).toContain(`\t\t\t1\n\t\t)\n\t});`);
    });

    test('comments in the edited object survive', async () => {
        const { out } = await formattedAdd(
            `$saw('a3', {\n  sync: 4, // note|\n  shape: 1 /* x */,\n}).out();`,
            'fm',
        );
        expect(out).toContain('// note');
        expect(out).toContain('/* x */');
        expect(out).toContain(`fm: $slider('fm', 0, -5, 5)`);
    });

    test('only the edited statement is reformatted', async () => {
        const { out } = await formattedAdd(
            `const   a  =  1\n$saw('a3', {shape: 2.5|}).out()\nconst   b  =  2`,
            'fm',
        );
        expect(out).toBe(
            `const   a  =  1\n$saw('a3', { shape: 2.5, fm: $slider('fm', 0, -5, 5) }).out();\nconst   b  =  2`,
        );
    });

    test('CRLF sources keep CRLF and an exact caret', async () => {
        const { out, fix } = await formattedAdd(
            `$saw('a3', {\r\n  shape: 2 // note|\r\n}).out();\r\n`,
            'fm',
        );
        expect(out).not.toMatch(/[^\r]\n/);
        const at = fix.span.start + fix.caretOffset;
        expect(out.slice(out.lastIndexOf('$slider(', at), at)).toBe(
            `$slider('fm', 0, -5, 5)`,
        );
    });

    test('a statement with a syntax error keeps the checked raw edit', async () => {
        const text = `$saw('a3', { shape: 2.5| }).out(\n$noise('white').out();`;
        const { source, fixes } = fixesAt(text);
        const raw = addFix(fixes, 'fm');
        const fix = await formatControlQuickFix(source, raw, PRETTIER);
        expect(fix).toBe(raw);
        expect(apply(source, fix)).toContain(
            `{ shape: 2.5, fm: $slider('fm', 0, -5, 5) }`,
        );
    });

    test('a multi-line template literal is never reindented', async () => {
        const text = `function f() {\n  $saw('a3', { shape: 2.5| }).out(\`a\nb\`);\n}`;
        const { source, fixes } = fixesAt(text);
        const raw = addFix(fixes, 'fm');
        const fix = await formatControlQuickFix(source, raw, PRETTIER);
        expect(apply(source, fix)).toContain('`a\nb`');
    });
});
