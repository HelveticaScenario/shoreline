/**
 * The check that keeps the control quick-fixes from breaking a patch's
 * syntax: an edit is rejected when it adds a parse error.
 */
import ts from 'typescript';

import { parseDsl, parseTransient } from '../../shared/dsl/ast/parse';
import type { TextEdit } from './listEdits';

/** Parse errors of `source` read as a JavaScript patch. */
export function syntaxErrors(
    source: string,
): readonly ts.DiagnosticWithLocation[] {
    return parseErrors(parseDsl(source));
}

/** Parse errors of an already parsed `file`. */
export function parseErrors(
    file: ts.SourceFile,
): readonly ts.DiagnosticWithLocation[] {
    // The parser's own error list: what a Program's syntactic diagnostics
    // return, without building a Program.
    return (
        (file as unknown as { parseDiagnostics?: ts.DiagnosticWithLocation[] })
            .parseDiagnostics ?? []
    );
}

/**
 * Whether applying `edit` to `source` adds a parse error. Errors the source
 * already has elsewhere, shifted by the edit, do not count; errors inside
 * the replaced span may disappear.
 */
export function introducesSyntaxErrors(
    source: string,
    edit: TextEdit,
    before: readonly ts.DiagnosticWithLocation[] = syntaxErrors(source),
): boolean {
    const { start, end } = edit.span;
    const delta = edit.newText.length - (end - start);
    const known = new Set<string>();
    for (const d of before) {
        if (d.start + d.length <= start) {
            known.add(`${d.start}:${d.code}`);
        } else if (d.start >= end) {
            known.add(`${d.start + delta}:${d.code}`);
        }
    }
    const after = parseErrors(
        parseTransient(
            source.slice(0, start) + edit.newText + source.slice(end),
        ),
    );
    return after.some((d) => !known.has(`${d.start}:${d.code}`));
}
