/**
 * Text edits the control quick-fixes are built from, and the check that keeps
 * them from breaking the patch's syntax.
 *
 * List edits rebuild the separators of the list they extend: the commas in
 * each gap between elements are replaced by exactly one, and those before the
 * first element dropped, so stray commas (`f(a,, b)`) and missing ones
 * (`{ a: 1 b: 2 }`) in that list are repaired. Comments in a gap are kept.
 */
import ts from 'typescript';

export interface TextEdit {
    /** Half-open [start, end) replacement span in the source. */
    span: { start: number; end: number };
    newText: string;
}

export interface ListEdit extends TextEdit {
    /** Offset within newText where the appended element begins. */
    elementOffset: number;
}

/** A block or line comment; the group keeps comments in `split` output. */
const COMMENT = /(\/\*[\s\S]*?\*\/|\/\/[^\n]*)/;
const COMMENTS = new RegExp(COMMENT.source, 'g');

export function appendCallArgument(
    file: ts.SourceFile,
    call: ts.CallExpression,
    element: string,
): ListEdit | null {
    const children = call.getChildren(file);
    const open = children.find((c) => c.kind === ts.SyntaxKind.OpenParenToken);
    const close = children.findLast(
        (c) => c.kind === ts.SyntaxKind.CloseParenToken,
    );
    if (!open || !close || close.getStart(file) < open.getEnd()) {
        return null;
    }
    return appendListElement(
        file.text,
        open.getEnd(),
        close.getStart(file),
        ranges(call.arguments, file),
        element,
        '',
    );
}

export function appendObjectProperty(
    file: ts.SourceFile,
    obj: ts.ObjectLiteralExpression,
    property: string,
): ListEdit | null {
    return appendListElement(
        file.text,
        obj.getStart(file) + 1,
        obj.getEnd() - 1,
        ranges(obj.properties, file),
        property,
        ' ',
    );
}

/**
 * Append `element` to the comma-separated list whose contents run from
 * `open` (just past the opening bracket) to `close` (the closing bracket),
 * with `elements` the existing elements' non-trivia ranges in order and
 * `pad` the space inside the brackets of a list that was empty (`{ a }`).
 * Null when a gap holds something other than whitespace, commas, and
 * comments.
 */
function appendListElement(
    source: string,
    open: number,
    close: number,
    elements: readonly { start: number; end: number }[],
    element: string,
    pad: string,
): ListEdit | null {
    const gaps: string[] = [];
    let at = open;
    for (const e of elements) {
        gaps.push(source.slice(at, e.start));
        at = e.end;
    }
    const trailing = source.slice(at, close);
    if ([...gaps, trailing].some((g) => !/^[\s,]*$/.test(withoutComments(g)))) {
        return null;
    }

    if (elements.length === 0) {
        const rest = withoutCommas(trailing);
        return {
            elementOffset: pad.length,
            newText: `${pad}${element}${rest.trim() === '' ? pad : rest}`,
            span: { end: close, start: open },
        };
    }
    let text = '';
    gaps.forEach((gap, i) => {
        text += i === 0 ? withoutCommas(gap) : separator(gap);
        text += source.slice(elements[i].start, elements[i].end);
    });
    // A trailing comma stays trailing, after the new element.
    const trailingComma = withoutComments(trailing).includes(',');
    const elementOffset = text.length + 2;
    text += `, ${element}${trailingComma ? ',' : ''}${withoutCommas(trailing)}`;
    return { elementOffset, newText: text, span: { end: close, start: open } };
}

/** One comma, then the gap's whitespace and comments, or a space. */
function separator(gap: string): string {
    const rest = withoutCommas(gap);
    return rest === '' || /^\S/.test(rest) ? `, ${rest}` : `,${rest}`;
}

function withoutComments(gap: string): string {
    return gap.replace(COMMENTS, '');
}

/** `gap` with its commas removed, its comments kept as written. */
function withoutCommas(gap: string): string {
    return gap
        .split(COMMENT)
        .map((part, i) => (i % 2 === 1 ? part : part.replace(/,/g, '')))
        .join('');
}

function ranges(nodes: readonly ts.Node[], file: ts.SourceFile) {
    return nodes.map((n) => ({ end: n.getEnd(), start: n.getStart(file) }));
}

/** Parse errors of `source` read as a JavaScript patch. */
export function syntaxErrors(
    source: string,
): readonly ts.DiagnosticWithLocation[] {
    return parseErrors(
        ts.createSourceFile(
            'patch.js',
            source,
            ts.ScriptTarget.Latest,
            false,
            ts.ScriptKind.JS,
        ),
    );
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
    const after = syntaxErrors(
        source.slice(0, start) + edit.newText + source.slice(end),
    );
    return after.some((d) => !known.has(`${d.start}:${d.code}`));
}
