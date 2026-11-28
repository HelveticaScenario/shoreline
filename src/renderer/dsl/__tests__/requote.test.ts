import { describe, expect, test } from 'vitest';

import schemas from '@modular/core/schemas.json';
import type { ModuleSchema } from '@modular/core';
import type { QuoteStyle } from '../../../shared/ipcTypes';
import { formatDsl } from '../../components/monaco/formattingProvider';
import { DEFAULT_PATCH } from '../../app/buffers';
import { GLOBAL_ENTRIES } from '../../../shared/dsl/globalDocs';
import { TYPE_DOCS } from '../../../shared/dsl/typeDocs';
import {
    VIDEO_CHAIN_EXAMPLES,
    VIDEO_DOCS,
    VIDEO_INTRO,
} from '../../../shared/dsl/videoDocs';
import { requote, requoteMarkdown } from '../requote';

const STYLES: QuoteStyle[] = ['single', 'double', 'backtick'];

describe('requote', () => {
    test('quotes strings for each style and keeps the layout', () => {
        const code = `$hzToV(440)    // A4\n$note("C4")  // Middle C`;
        expect(requote(code, 'single')).toBe(
            `$hzToV(440)    // A4\n$note('C4')  // Middle C`,
        );
        expect(requote(code, 'double')).toBe(code);
        expect(requote(code, 'backtick')).toBe(
            '$hzToV(440)    // A4\n$note(`C4`)  // Middle C',
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
        expect(requote(`a = 'it\\'s'`, 'single')).toBe(`a = "it's"`);
    });

    test('keeps every escape but the quotes it changes', () => {
        expect(requote(`a = "it\\'s"`, 'single')).toBe(`a = "it\\'s"`);
        expect(requote(`a = '\\$x \\a \\8'`, 'double')).toBe(
            `a = "\\$x \\a \\8"`,
        );
    });

    test('quotes a directive only when it holds no quotes', () => {
        expect(requote(`'C4'   // Middle C`, 'double')).toBe(
            `"C4"   // Middle C`,
        );
        expect(requote(`"C4"`, 'backtick')).toBe(`'C4'`);
        expect(requote(`"it's"`, 'single')).toBe(`"it's"`);
    });

    test('keeps a bare string statement quoted in backtick mode', () => {
        expect(requote(`f()\n'x'`, 'backtick')).toBe(`f()\n'x'`);
    });

    test('leaves unterminated literals as written', () => {
        expect(requote(`a = 'c4 e4`, 'double')).toBe(`a = 'c4 e4`);
        expect(requote('a = `c4 e4', 'single')).toBe('a = `c4 e4');
    });

    test('keeps a template with an octal-like escape', () => {
        expect(requote('a = `\\1`', 'single')).toBe('a = `\\1`');
        expect(requote('a = `\\\\1`', 'single')).toBe(`a = '\\\\1'`);
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

    test('agrees with Format Document on every other example shown', async () => {
        // Formatted first, so only the quotes can differ.
        const codes = [
            DEFAULT_PATCH,
            "$wavetable('c3', 'builtin/wavetable.wav', 2.5)",
            ...Object.values(TYPE_DOCS).flatMap((t) => [
                ...t.examples,
                ...(t.methods ?? []).flatMap((m) =>
                    m.example ? [m.example] : [],
                ),
            ]),
            ...GLOBAL_ENTRIES.flatMap((g) =>
                [
                    ...g.docs.examples,
                    ...g.docs.sections.flatMap((section) => section.examples),
                ].map((e) => e.code),
            ),
            ...VIDEO_INTRO.examples,
            ...VIDEO_CHAIN_EXAMPLES,
            ...VIDEO_DOCS.flatMap((d) => d.examples),
        ];
        let checked = 0;
        for (const code of codes) {
            const formatted = await formatDsl(code, { quotes: 'double' }).catch(
                () => null,
            );
            if (formatted === null) {
                continue;
            }
            checked++;
            for (const quotes of STYLES) {
                expect(requote(formatted, quotes), code).toBe(
                    await formatDsl(code, { quotes }),
                );
            }
        }
        expect(checked).toBeGreaterThan(200);
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

    test('an empty code block ends at its own fence', () => {
        const markdown = ['```js', '```', "Use `'c4'` here."].join('\n');
        expect(requoteMarkdown(markdown, 'double')).toBe(markdown);
    });
});
