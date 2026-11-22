import { describe, expect, test } from 'vitest';

import schemas from '@modular/core/schemas.json';
import type { ModuleSchema } from '@modular/core';
import type { QuoteStyle } from '../../../shared/ipcTypes';
import { formatDsl } from '../../components/monaco/formattingProvider';
import { requote, requoteMarkdown } from '../requote';

const STYLES: QuoteStyle[] = ['single', 'double', 'backtick'];

describe('requote', () => {
    test('quotes strings for each style and keeps the layout', () => {
        const code = `$hz(440)    // A4\n$note("C4")  // Middle C`;
        expect(requote(code, 'single')).toBe(
            `$hz(440)    // A4\n$note('C4')  // Middle C`,
        );
        expect(requote(code, 'double')).toBe(code);
        expect(requote(code, 'backtick')).toBe(
            '$hz(440)    // A4\n$note(`C4`)  // Middle C',
        );
    });

    test('keeps strings the grammar requires quoted', () => {
        const code = [
            `import a from "mod"`,
            `const o = { "a-b": 1, ["k"]: "v" }`,
            `function f() { 'use strict' }`,
        ].join('\n');
        expect(requote(code, 'backtick')).toBe(
            [
                `import a from 'mod'`,
                "const o = { 'a-b': 1, [`k`]: `v` }",
                `function f() { 'use strict' }`,
            ].join('\n'),
        );
    });

    test('picks the quote that needs fewer escapes', () => {
        expect(requote(`a = "it's"`, 'single')).toBe(`a = "it's"`);
        expect(requote(`a = 'say "hi"'`, 'double')).toBe(`a = 'say "hi"'`);
        expect(requote(`a = "it\\'s"`, 'single')).toBe(`a = "it's"`);
    });

    test('turns plain template literals into strings outside backtick mode', () => {
        expect(requote('a = `c4 e4`', 'single')).toBe(`a = 'c4 e4'`);
        const kept = ['a = `c4 ${n}`', 'b = `c4\ne4`', 'c = $`c4 e4`'].join(
            '\n',
        );
        expect(requote(kept, 'double')).toBe(kept);
    });

    test('agrees with Format Document on formatted examples', async () => {
        const codes = (schemas as ModuleSchema[]).flatMap((s) =>
            [
                ...s.docs.examples,
                ...s.docs.sections.flatMap((section) => section.examples),
            ].map((e) => e.code),
        );
        expect(codes.length).toBeGreaterThan(0);
        for (const quotes of STYLES) {
            for (const code of codes) {
                expect(requote(code, quotes), code).toBe(
                    await formatDsl(code, { quotes }),
                );
            }
        }
    });
});

describe('requoteMarkdown', () => {
    test('requotes JavaScript code blocks only', () => {
        const markdown = [
            "Takes `'c4'`.",
            '```typescript',
            "f(x: 'a'): void",
            '```',
            '```js',
            "out: $sine('a3')",
            '```',
        ].join('\n');
        expect(requoteMarkdown(markdown, 'backtick')).toBe(
            markdown.replace("out: $sine('a3')", 'out: $sine(`a3`)'),
        );
    });
});
