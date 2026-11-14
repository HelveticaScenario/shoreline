import { describe, expect, test } from 'vitest';

import type { QuoteStyle } from '../../../../shared/ipcTypes';
import { templateBody } from '../quoteStyle';
import { formatDsl, resolvePrettierOptions } from '../formattingProvider';

function format(code: string, quotes?: QuoteStyle): Promise<string> {
    return formatDsl(code, { quotes });
}

describe('backtickStrings', () => {
    test('quotes strings in single quotes by default', async () => {
        expect(await format(`$cycle("c4 e4").out()`)).toBe(
            `$cycle('c4 e4').out()`,
        );
        expect(await format(`$cycle('c4 e4').out()`, 'double')).toBe(
            `$cycle("c4 e4").out()`,
        );
    });

    test('quotes decides over a raw singleQuote', () => {
        expect(resolvePrettierOptions({ singleQuote: false })).toMatchObject({
            quotes: 'double',
            singleQuote: false,
        });
        expect(
            resolvePrettierOptions({ quotes: 'backtick', singleQuote: false }),
        ).toMatchObject({ quotes: 'backtick', singleQuote: true });
    });

    test('prints string arguments and values as template literals', async () => {
        expect(
            await format(
                `$saw($slider('root', "c3", 'c2', 'c5')).out()`,
                'backtick',
            ),
        ).toBe('$saw($slider(`root`, `c3`, `c2`, `c5`)).out()');
    });

    test('keeps strings the grammar requires quoted', async () => {
        const code = [
            `import a from 'mod'`,
            `export { a as 'b' } from 'mod'`,
            `const o = { 'a-b': 1, ['k']: 'v' }`,
            `class C { 'm-n'() {} }`,
            `function f() { 'use strict' }`,
        ].join('\n');
        expect(await format(code, 'backtick')).toBe(
            [
                `import a from 'mod'`,
                `export { a as 'b' } from 'mod'`,
                "const o = { 'a-b': 1, [`k`]: `v` }",
                `class C {`,
                `  'm-n'() {}`,
                `}`,
                `function f() {`,
                `  'use strict'`,
                `}`,
            ].join('\n'),
        );
    });

    test('keeps comments in place', async () => {
        expect(
            await format(`// lead\n$cycle('c4') // trail\n`, 'backtick'),
        ).toBe('// lead\n$cycle(`c4`) // trail');
    });
});

describe('template literals in single or double mode', () => {
    test('become quoted strings when they hold plain text', async () => {
        expect(await format('$cycle(`c4 e4`).out()')).toBe(
            `$cycle('c4 e4').out()`,
        );
        expect(await format('$cycle(`c4 e4`).out()', 'double')).toBe(
            `$cycle("c4 e4").out()`,
        );
    });

    test('re-escape quotes and drop backtick-only escapes', async () => {
        expect(await format("const a = `it's \\` \\${x}`")).toBe(
            `const a = "it's \` \${x}"`,
        );
        expect(await format('const a = `say "hi"`')).toBe(
            `const a = 'say "hi"'`,
        );
        expect(await format('const a = `tab\\there`')).toBe(
            `const a = 'tab\\there'`,
        );
    });

    test('stay template literals when they interpolate, span lines or are tagged', async () => {
        const code = [
            'const a = `c4 ${n}`',
            'const b = `c4\ne4`',
            'const c = String.raw`c4`',
            'const d = $`c4 e4`',
        ].join('\n');
        expect(await format(code)).toBe(code);
    });

    test('stay template literals standing alone as a statement', async () => {
        expect(await format('function f() {\n  `use strict`\n}')).toBe(
            'function f() {\n  ;`use strict`\n}',
        );
    });
});

describe('templateBody', () => {
    test('escapes backticks and interpolation openers', () => {
        expect(templateBody('a `b` ${c} $d')).toBe('a \\`b\\` \\${c} $d');
    });

    test('drops quote escapes and keeps the others', () => {
        expect(templateBody(`it\\'s \\"q\\" \\n \\\\ \\0`)).toBe(
            `it's "q" \\n \\\\ \\0`,
        );
    });

    test('refuses escapes a template literal forbids', () => {
        expect(templateBody('\\01')).toBeNull();
        expect(templateBody('\\8')).toBeNull();
    });
});
