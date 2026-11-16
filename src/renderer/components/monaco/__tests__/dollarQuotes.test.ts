import ts from 'typescript';
import { describe, expect, test } from 'vitest';

import {
    backtickedPattern,
    DOLLAR_QUOTE_CODE,
    templateBody,
    withDollarQuoteDiagnostics,
    withoutDollarQuoteTypeErrors,
} from '../dollarQuotes';

const LIB = `
interface Pattern { out(): void }
declare function $(source: string): Pattern;
declare function $(strings: TemplateStringsArray, ...values: unknown[]): Pattern;
`;

function diagnosticsOf(source: string) {
    const files: Record<string, string> = { '/a.js': source, '/lib.d.ts': LIB };
    const service = ts.createLanguageService({
        fileExists: (f) => f in files || ts.sys.fileExists(f),
        getCompilationSettings: () => ({
            allowJs: true,
            checkJs: true,
            lib: ['lib.esnext.d.ts'],
            noEmit: true,
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
    });
    const sourceFile = service.getProgram()!.getSourceFile('/a.js')!;
    const summary = (d: ts.Diagnostic) => ({
        code: d.code,
        text: source.slice(d.start, d.start! + d.length!),
        message: ts.flattenDiagnosticMessageText(d.messageText, '\n'),
    });
    return {
        syntactic: withDollarQuoteDiagnostics(
            ts,
            sourceFile,
            service.getSyntacticDiagnostics('/a.js'),
        ).map(summary),
        semantic: withoutDollarQuoteTypeErrors(
            ts,
            sourceFile,
            service.getSemanticDiagnostics('/a.js'),
        ).map(summary),
    };
}

describe("$'…' diagnostics", () => {
    test('a single-quoted pattern says to use backticks', () => {
        expect(diagnosticsOf("const p = $'c e g'.out();")).toEqual({
            syntactic: [
                {
                    code: DOLLAR_QUOTE_CODE,
                    text: "$'c e g'",
                    message:
                        "Patterns use backticks: write $`c e g`, or call $('c e g').",
                },
            ],
            semantic: [],
        });
    });

    test('a double-quoted pattern as an argument', () => {
        const { syntactic } = diagnosticsOf('[1, $"c e", 2];');
        expect(syntactic).toEqual([
            {
                code: DOLLAR_QUOTE_CODE,
                text: '$"c e"',
                message:
                    'Patterns use backticks: write $`c e`, or call $("c e").',
            },
        ]);
    });

    test('unrelated errors stay', () => {
        const { syntactic, semantic } = diagnosticsOf("$'c'; missing.x;");
        expect(syntactic.map((d) => d.text)).toEqual(["$'c'"]);
        expect(semantic.map((d) => d.text)).toEqual(['missing']);
    });

    test('backticks, calls and spaced strings are left alone', () => {
        expect(
            diagnosticsOf("$`c e`.out(); $('c').out(); const $a = 'x';"),
        ).toEqual({ syntactic: [], semantic: [] });
    });

    test("an unclosed string keeps the parser's own error", () => {
        const { syntactic } = diagnosticsOf("const p = $'c e g\nx;");
        expect(syntactic.map((d) => d.code)).not.toContain(DOLLAR_QUOTE_CODE);
        expect(syntactic.map((d) => d.message)).toContain(
            'Unterminated string literal.',
        );
    });
});

describe('templateBody', () => {
    test('escapes what a template would read', () => {
        expect(templateBody("a\\'b`c${d}")).toBe("a'b\\`c\\${d}");
    });

    test('keeps an escaped backslash before a quote', () => {
        expect(templateBody('x\\\\"y')).toBe('x\\\\"y');
    });
});

describe('backtickedPattern', () => {
    test('rewrites either quote style', () => {
        expect(backtickedPattern("$'c e g'")).toBe('$`c e g`');
        expect(backtickedPattern('$"c e"')).toBe('$`c e`');
    });

    test('declines text that is not a quoted pattern', () => {
        expect(backtickedPattern("$'c e")).toBeNull();
        expect(backtickedPattern('$\'c"')).toBeNull();
        expect(backtickedPattern('const x')).toBeNull();
    });
});
