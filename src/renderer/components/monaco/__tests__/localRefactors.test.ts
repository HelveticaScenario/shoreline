import ts from 'typescript';
import { describe, expect, test } from 'vitest';

import { localRefactorEdits, localRefactors } from '../localRefactors';
import type { RefactorStyle } from '../localRefactors';

const STYLE: RefactorStyle = {
    convertTabsToSpaces: true,
    eol: '\n',
    semi: true,
    singleQuote: true,
    tabSize: 2,
};

/** A language service over in-memory scripts, checked as JS like the
 *  editor's patch buffers. */
function serviceFor(files: Record<string, string>): ts.LanguageService {
    const host: ts.LanguageServiceHost = {
        fileExists: (f) => f in files || ts.sys.fileExists(f),
        getCompilationSettings: () => ({
            allowJs: true,
            checkJs: true,
            lib: ['lib.esnext.d.ts'],
            noEmit: true,
            target: ts.ScriptTarget.ES2020,
        }),
        getCurrentDirectory: () => '/',
        getDefaultLibFileName: (o) => ts.getDefaultLibFilePath(o),
        getScriptFileNames: () => Object.keys(files),
        getScriptSnapshot: (f) => {
            const text = files[f] ?? ts.sys.readFile(f);
            return text === undefined
                ? undefined
                : ts.ScriptSnapshot.fromString(text);
        },
        getScriptVersion: () => '1',
        readFile: (f) => files[f] ?? ts.sys.readFile(f),
    };
    return ts.createLanguageService(host);
}

/** `[` and `]` mark the selection; returns the source without them. */
function selection(text: string): {
    source: string;
    start: number;
    end: number;
} {
    const start = text.indexOf('[');
    const end = text.indexOf(']') - 1;
    return { end, source: text.replace('[', '').replace(']', ''), start };
}

function applyEdits(
    source: string,
    edits: { start: number; end: number; newText: string }[],
): string {
    return [...edits]
        .sort((a, b) => b.start - a.start)
        .reduce(
            (text, e) => text.slice(0, e.start) + e.newText + text.slice(e.end),
            source,
        );
}

const FILE = '/patch.js';

describe('localRefactors', () => {
    test('offers extract to constant and function for a selected expression', () => {
        const { source, start, end } = selection(
            `function voice(n) {\n  return [n * 2 + 1];\n}\n`,
        );
        const kinds = localRefactors(
            serviceFor({ [FILE]: source }),
            FILE,
            start,
            end,
            false,
        ).map((r) => r.kind);
        expect(kinds).toContain('refactor.extract.constant');
        expect(kinds).toContain('refactor.extract.function');
    });

    test('offers only extract and inline refactors', () => {
        // Arrow conversion, template strings and the like are left out.
        const source = `const f = (a) => a + 'x' + 'y';\n`;
        const service = serviceFor({ [FILE]: source });
        for (let at = 0; at < source.length; at++) {
            for (const r of localRefactors(service, FILE, at, at, true)) {
                expect(r.kind).toMatch(/^refactor\.(extract|inline)\./);
            }
        }
    });

    test('an explicit request extracts the expression at an empty selection', () => {
        const source = `const x = 3 * 4;\n`;
        const at = source.indexOf('3');
        const service = serviceFor({ [FILE]: source });
        expect(localRefactors(service, FILE, at, at, false)).toEqual([]);
        expect(
            localRefactors(service, FILE, at, at, true).map((r) => r.kind),
        ).toContain('refactor.extract.constant');
    });

    test('extract to constant names the new constant and points rename at it', () => {
        const { source, start, end } = selection(
            `function voice(n) {\n  return [n * 2] + 1;\n}\n`,
        );
        const service = serviceFor({ [FILE]: source });
        const extract = localRefactors(service, FILE, start, end, false).find(
            (r) => r.kind === 'refactor.extract.constant',
        )!;
        const result = localRefactorEdits(
            ts,
            service,
            FILE,
            start,
            end,
            extract.refactor,
            extract.action,
            STYLE,
        )!;
        const out = applyEdits(source, result.edits);
        expect(out).toBe(
            `function voice(n) {\n  const newLocal = n * 2;\n  return newLocal + 1;\n}\n`,
        );
        expect(out.slice(result.renameOffset, result.renameOffset! + 8)).toBe(
            'newLocal',
        );
    });

    test('a CRLF buffer gets CRLF edits and a rename offset into them', () => {
        const { source, start, end } = selection(
            `function voice(n) {\r\n  return [n * 2] + 1;\r\n}\r\n`,
        );
        const service = serviceFor({ [FILE]: source });
        const extract = localRefactors(service, FILE, start, end, false).find(
            (r) => r.kind === 'refactor.extract.constant',
        )!;
        const result = localRefactorEdits(
            ts,
            service,
            FILE,
            start,
            end,
            extract.refactor,
            extract.action,
            { ...STYLE, eol: '\r\n' },
        )!;
        const out = applyEdits(source, result.edits);
        expect(out).not.toMatch(/[^\r]\n/);
        expect(out.slice(result.renameOffset, result.renameOffset! + 8)).toBe(
            'newLocal',
        );
    });

    test('inline variable replaces every use in the buffer', () => {
        const source = `function f() {\n  const g = 2;\n  return g * g;\n}\n`;
        const at = source.indexOf('g =');
        const service = serviceFor({ [FILE]: source });
        const inline = localRefactors(service, FILE, at, at, true).find(
            (r) => r.kind === 'refactor.inline.variable',
        )!;
        expect(inline).toBeDefined();
        const result = localRefactorEdits(
            ts,
            service,
            FILE,
            at,
            at,
            inline.refactor,
            inline.action,
            STYLE,
        )!;
        expect(applyEdits(source, result.edits)).toBe(
            `function f() {\n  return 2 * 2;\n}\n`,
        );
    });

    test('inline is not offered for a variable another buffer uses', () => {
        // Top-level script variables are shared by every open buffer.
        const source = `const g = 2;\ng * 3;\n`;
        const at = source.indexOf('g =');
        const inlineKinds = (files: Record<string, string>) =>
            localRefactors(serviceFor(files), FILE, at, at, true)
                .map((r) => r.kind)
                .filter((k) => k === 'refactor.inline.variable');
        expect(inlineKinds({ [FILE]: source })).toHaveLength(1);
        expect(
            inlineKinds({ '/other.js': `g + 1;\n`, [FILE]: source }),
        ).toEqual([]);
    });
});
