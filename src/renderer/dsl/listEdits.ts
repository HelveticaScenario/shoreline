/**
 * Source edits that append to a bracketed, comma-separated list (an object
 * literal's properties, a call's arguments, an array's elements) laid out
 * the way Prettier would print it, without reformatting anything else in
 * the file.
 *
 * Prettier keeps a list expanded when the original has a newline between
 * its opening bracket and first element; otherwise it prints the list on
 * one line when that fits `printWidth`, and an object one property per line
 * when it doesn't.
 *
 * Each edit also repairs the separators of the list it extends: a gap
 * between elements with no comma or several gets exactly one, and commas
 * before the first element or doubled after the last are dropped, so
 * `f(a,, b)` and `{ a: 1 b: 2 }` never carry into the result. Comments in a
 * gap are kept, and a gap whose commas are already right is left as written.
 */
import ts from 'typescript';

import { parseTransient } from '../../shared/dsl/ast/parse';
import type { PrettierConfig } from '../../shared/ipcTypes';

/** The Prettier options generated code follows. */
export interface CodeStyle {
    printWidth: number;
    tabWidth: number;
    useTabs: boolean;
    bracketSpacing: boolean;
    /** Whether expanded objects end with a trailing comma. */
    trailingComma: boolean;
    /** Delimiter for generated string literals. */
    quote: "'" | '"';
}

/** Unset options fall back to Prettier's own defaults. */
export function codeStyleFromPrettier(config: PrettierConfig): CodeStyle {
    return {
        bracketSpacing: config.bracketSpacing !== false,
        printWidth: config.printWidth ?? 80,
        quote: config.singleQuote === true ? "'" : '"',
        tabWidth: config.tabWidth ?? 2,
        trailingComma: (config.trailingComma ?? 'all') !== 'none',
        useTabs: config.useTabs === true,
    };
}

export interface TextEdit {
    /** Half-open [start, end) replacement span in the source. */
    span: { start: number; end: number };
    newText: string;
}

export interface ListEdit extends TextEdit {
    /** Offset within newText where the appended element begins. */
    elementOffset: number;
}

/** The contents of a list: from just past its opening bracket to its
 *  closing bracket, and its elements in order. */
interface ListShape {
    open: number;
    close: number;
    elements: readonly ts.Node[];
    /** Space inside the brackets of a one-element list (`{ a }`). */
    pad: string;
}

/** Add `prop` (e.g. `fm: 1`) as the last property of `obj`; null when the
 *  object has no closing brace to append before. */
export function appendObjectProperty(
    file: ts.SourceFile,
    obj: ts.ObjectLiteralExpression,
    prop: string,
    layout: CodeStyle,
): ListEdit | null {
    const shape = objectShape(layout);
    if (!shape(obj, file)) {
        return null;
    }
    return withRepairedSeparators(
        file,
        obj,
        (o, f) => shape(o, f)!,
        (f, o) => {
            const list = shape(o, f)!;
            const edit = appendToList(f, list, prop);
            return isExpanded(f, list) || fitsOnLine(f.text, edit, layout)
                ? edit
                : (expandObject(f, o, prop, layout) ?? edit);
        },
    );
}

/** Append `text` as the last argument of `call`; null when the call has no
 *  parentheses to append inside. */
export function appendArgument(
    file: ts.SourceFile,
    call: ts.CallExpression,
    text: string,
): ListEdit | null {
    if (!argumentShape(call, file)) {
        return null;
    }
    return withRepairedSeparators(
        file,
        call,
        (c, f) => argumentShape(c, f)!,
        (f, c) => appendToList(f, argumentShape(c, f)!, text),
    );
}

/**
 * Append a new trailing object argument `{ prop }` to `call`, its
 * `elementOffset` that of `prop`. Too wide for one line, the object breaks
 * the way Prettier hugs a last-argument object: `f(a, {⏎  prop,⏎})`.
 */
export function appendObjectArgument(
    file: ts.SourceFile,
    call: ts.CallExpression,
    prop: string,
    layout: CodeStyle,
): ListEdit | null {
    const pad = layout.bracketSpacing ? ' ' : '';
    const element = `{${pad}${prop}${pad}}`;
    const inline = appendArgument(file, call, element);
    if (!inline) {
        return null;
    }
    const at = inline.elementOffset;
    // The edit may span several lines (an expanded call, a repaired
    // multi-line argument list); only the line holding the object counts.
    const result = applyEdit(file.text, inline);
    const elementAt = inline.span.start + at;
    const here = { newText: '', span: { end: elementAt, start: elementAt } };
    if (fitsOnLine(result, here, layout)) {
        return { ...inline, elementOffset: at + 1 + pad.length };
    }
    const nl = eolOf(file.text);
    const outer = indentAt(result, elementAt);
    const head = `{${nl}${outer}${indentUnit(layout)}`;
    const hugged = `${head}${prop}${layout.trailingComma ? ',' : ''}${nl}${outer}}`;
    return {
        elementOffset: at + head.length,
        newText:
            inline.newText.slice(0, at) +
            hugged +
            inline.newText.slice(at + element.length),
        span: inline.span,
    };
}

/** Append `text` as the last element of `array`; null when the array has
 *  no closing bracket to append before. */
export function appendArrayElement(
    file: ts.SourceFile,
    array: ts.ArrayLiteralExpression,
    text: string,
): ListEdit | null {
    const shape = (a: ts.ArrayLiteralExpression, f: ts.SourceFile) =>
        bracketShape(a, a.elements, f, '', ts.SyntaxKind.CloseBracketToken);
    if (!shape(array, file)) {
        return null;
    }
    return withRepairedSeparators(
        file,
        array,
        (a, f) => shape(a, f)!,
        (f, a) => appendToList(f, shape(a, f)!, text),
    );
}

const objectShape =
    (layout: CodeStyle) => (o: ts.ObjectLiteralExpression, f: ts.SourceFile) =>
        bracketShape(
            o,
            o.properties,
            f,
            layout.bracketSpacing ? ' ' : '',
            ts.SyntaxKind.CloseBraceToken,
        );

/** The list inside a bracketed literal, or null when the parser found no
 *  closing bracket, as in a literal still being typed. */
function bracketShape(
    node: ts.Node,
    elements: readonly ts.Node[],
    file: ts.SourceFile,
    pad: string,
    closeKind: ts.SyntaxKind,
): ListShape | null {
    const close = node.getLastToken(file);
    if (!close || close.kind !== closeKind) {
        return null;
    }
    return {
        close: close.getStart(file),
        elements,
        open: node.getStart(file) + 1,
        pad,
    };
}

function argumentShape(
    call: ts.CallExpression,
    file: ts.SourceFile,
): ListShape | null {
    const children = call.getChildren(file);
    const open = children.find((c) => c.kind === ts.SyntaxKind.OpenParenToken);
    const close = children.findLast(
        (c) => c.kind === ts.SyntaxKind.CloseParenToken,
    );
    if (!open || !close || close.getStart(file) < open.getEnd()) {
        return null;
    }
    return {
        close: close.getStart(file),
        elements: call.arguments,
        open: open.getEnd(),
        pad: '',
    };
}

/**
 * Append `element` to a list whose separators are in order, keeping its
 * layout: on its own line in an expanded list, inline otherwise.
 */
function appendToList(
    file: ts.SourceFile,
    list: ListShape,
    element: string,
): ListEdit {
    const { open, close, elements, pad } = list;
    const source = file.text;
    if (elements.length === 0) {
        const inner = source.slice(open, close);
        // Only whitespace is replaced; a comment-bearing body is kept.
        if (inner.trim() === '') {
            return {
                elementOffset: pad.length,
                newText: `${pad}${element}${pad}`,
                span: { end: close, start: open },
            };
        }
        return {
            elementOffset: 0,
            newText: element,
            span: { end: close, start: close },
        };
    }

    const last = elements[elements.length - 1];
    // The last element's trailing comma, possibly after a comment.
    const comma = /^(?:\s|\/\*[\s\S]*?\*\/|\/\/[^\n]*)*,/.exec(
        source.slice(last.getEnd(), close),
    );
    const afterLast = comma ? last.getEnd() + comma[0].length : last.getEnd();

    if (isExpanded(file, list)) {
        // At the end of the last element's line, so a trailing line comment
        // stays attached to the element it annotates.
        const nl = eolOf(source);
        const eol = lineEnd(source, afterLast);
        const insertAt = eol < close ? eol : afterLast;
        const indent = indentAt(source, last.getStart(file));
        if (comma) {
            return {
                elementOffset: nl.length + indent.length,
                newText: `${nl}${indent}${element},`,
                span: { end: insertAt, start: insertAt },
            };
        }
        const tail = source.slice(last.getEnd(), insertAt);
        const newText = `,${tail}${nl}${indent}${element}`;
        return {
            elementOffset: newText.length - element.length,
            newText,
            span: { end: insertAt, start: last.getEnd() },
        };
    }
    return comma
        ? {
              elementOffset: 1,
              newText: ` ${element}`,
              span: { end: afterLast, start: afterLast },
          }
        : {
              elementOffset: 2,
              newText: `, ${element}`,
              span: { end: last.getEnd(), start: last.getEnd() },
          };
}

/** Whether a newline sits between the list's opening bracket and its first
 *  element, which Prettier keeps expanded. */
function isExpanded(file: ts.SourceFile, list: ListShape): boolean {
    const first = list.elements[0];
    return (
        first !== undefined &&
        file.text.slice(list.open, first.getStart(file)).includes('\n')
    );
}

/**
 * Rewrite `obj` one property per line with `prop` appended, or null when
 * the object holds comments a rebuild from property text would drop.
 */
function expandObject(
    file: ts.SourceFile,
    obj: ts.ObjectLiteralExpression,
    prop: string,
    layout: CodeStyle,
): ListEdit | null {
    const source = file.text;
    let residual = obj.getText(file);
    for (const p of obj.properties) {
        residual = residual.replace(p.getText(file), '');
    }
    if (/\/[/*]/.test(residual)) {
        return null;
    }
    const nl = eolOf(source);
    const outer = indentAt(source, obj.getStart(file));
    const inner = outer + indentUnit(layout);
    const existing = obj.properties
        .map((p) => `${inner}${p.getText(file)},${nl}`)
        .join('');
    const head = `{${nl}${existing}${inner}`;
    return {
        elementOffset: head.length,
        newText: `${head}${prop}${layout.trailingComma ? ',' : ''}${nl}${outer}}`,
        span: { end: obj.getEnd(), start: obj.getStart(file) },
    };
}

/**
 * Run `append` on `node` with its list's separators repaired first, as one
 * edit of the original source. `node` is found again in the repaired source
 * by its start, which a repair inside its brackets never moves, and its end,
 * shifted by the repair: calls in a chain share their start.
 */
function withRepairedSeparators<T extends ts.Node>(
    file: ts.SourceFile,
    node: T,
    shapeOf: (node: T, file: ts.SourceFile) => ListShape,
    append: (file: ts.SourceFile, node: T) => ListEdit,
): ListEdit {
    const repair = separatorRepair(file, shapeOf(node, file));
    if (!repair) {
        return append(file, node);
    }
    const repairedText = applyEdit(file.text, repair);
    const repaired = parseTransient(repairedText);
    const shift = repair.newText.length - (repair.span.end - repair.span.start);
    const again = nodeAt(
        repaired,
        node.getStart(file),
        node.getEnd() + shift,
        node.kind,
    ) as T | undefined;
    if (!again) {
        return append(file, node);
    }
    return compose(file.text, repair, append(repaired, again));
}

/**
 * The edit giving every gap of the list its right number of commas: none
 * before the first element, one between two, and at most one after the
 * last (none in an empty list). Null when every gap is right, or one holds
 * anything but whitespace, commas and comments.
 */
function separatorRepair(
    file: ts.SourceFile,
    { open, close, elements }: ListShape,
): TextEdit | null {
    const source = file.text;
    let changed = false;
    let text = '';
    let at = open;
    for (let i = 0; i <= elements.length; i++) {
        const element = elements[i];
        const gapEnd = element ? element.getStart(file) : close;
        const gap = source.slice(at, gapEnd);
        if (!/^[\s,]*$/.test(withoutComments(gap))) {
            return null;
        }
        const commas = withoutComments(gap).split(',').length - 1;
        const kind = i === 0 ? 'leading' : element ? 'between' : 'trailing';
        const right =
            kind === 'between'
                ? commas === 1
                : kind === 'trailing'
                  ? commas <= 1
                  : commas === 0;
        if (right) {
            text += gap;
        } else {
            changed = true;
            text +=
                kind === 'leading'
                    ? withoutCommas(gap)
                    : commas === 0
                      ? oneComma(gap)
                      : keepFirstComma(gap);
        }
        if (element) {
            text += source.slice(gapEnd, element.getEnd());
            at = element.getEnd();
        }
    }
    return changed
        ? { newText: text, span: { end: close, start: open } }
        : null;
}

/** A comma before `gap`, which has none, spaced from what follows. */
function oneComma(gap: string): string {
    return gap === '' || /^\S/.test(gap) ? `, ${gap}` : `,${gap}`;
}

/** `gap` with every comma after its first removed, comments untouched, so
 *  a comment stays on the side of the comma it was written on. */
function keepFirstComma(gap: string): string {
    let seen = false;
    return gap
        .split(COMMENT)
        .map((part, i) =>
            i % 2 === 1
                ? part
                : part.replace(/,/g, () => {
                      const keep = !seen;
                      seen = true;
                      return keep ? ',' : '';
                  }),
        )
        .join('');
}

/** `first` then `second` (an edit of `first`'s result) as one edit of
 *  `source`. */
function compose(source: string, first: TextEdit, second: ListEdit): ListEdit {
    const firstEnd = first.span.start + first.newText.length;
    const delta = first.newText.length - (first.span.end - first.span.start);
    const toSource = (pos: number, side: 'start' | 'end') => {
        if (pos <= first.span.start) {
            return pos;
        }
        if (pos >= firstEnd) {
            return pos - delta;
        }
        return side === 'start' ? first.span.start : first.span.end;
    };
    const start = Math.min(
        first.span.start,
        toSource(second.span.start, 'start'),
    );
    const end = Math.max(first.span.end, toSource(second.span.end, 'end'));
    const result = applyEdit(applyEdit(source, first), second);
    const secondDelta =
        second.newText.length - (second.span.end - second.span.start);
    return {
        elementOffset: second.span.start + second.elementOffset - start,
        newText: result.slice(start, end + delta + secondDelta),
        span: { end, start },
    };
}

function applyEdit(source: string, edit: TextEdit): string {
    return (
        source.slice(0, edit.span.start) +
        edit.newText +
        source.slice(edit.span.end)
    );
}

function nodeAt(
    file: ts.SourceFile,
    start: number,
    end: number,
    kind: ts.SyntaxKind,
): ts.Node | undefined {
    let found: ts.Node | undefined;
    const visit = (node: ts.Node) => {
        if (found || node.end < start || node.pos > start) {
            return;
        }
        if (
            node.kind === kind &&
            node.getStart(file) === start &&
            node.getEnd() === end
        ) {
            found = node;
            return;
        }
        ts.forEachChild(node, visit);
    };
    visit(file);
    return found;
}

/** A block or line comment; the group keeps comments in `split` output. */
const COMMENT = /(\/\*[\s\S]*?\*\/|\/\/[^\n]*)/;
const COMMENTS = new RegExp(COMMENT.source, 'g');

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

/** Whether the line holding a single-line edit stays within printWidth. */
export function fitsOnLine(
    source: string,
    edit: TextEdit,
    layout: CodeStyle,
): boolean {
    const line =
        source.slice(lineStart(source, edit.span.start), edit.span.start) +
        edit.newText +
        source.slice(edit.span.end, lineEnd(source, edit.span.end));
    return visualWidth(line, layout.tabWidth) <= layout.printWidth;
}

export function indentUnit(layout: CodeStyle): string {
    return layout.useTabs ? '\t' : ' '.repeat(layout.tabWidth);
}

function lineStart(source: string, pos: number): number {
    return source.lastIndexOf('\n', pos - 1) + 1;
}

/** Offset of the line break ending the line at `pos` (the `\r` of a
 *  `\r\n`), or the source length on the last line. */
export function lineEnd(source: string, pos: number): number {
    const end = source.indexOf('\n', pos);
    if (end === -1) {
        return source.length;
    }
    return end > pos && source[end - 1] === '\r' ? end - 1 : end;
}

/**
 * The source's line break. Inserted text uses it verbatim so an editor
 * model normalizes nothing and offsets into the inserted text stay exact.
 */
export function eolOf(source: string): string {
    return source.includes('\r\n') ? '\r\n' : '\n';
}

/** Leading whitespace of the line containing `pos`. */
export function indentAt(source: string, pos: number): string {
    const start = lineStart(source, pos);
    return /^[ \t]*/.exec(source.slice(start, lineEnd(source, start)))![0];
}

function visualWidth(text: string, tabWidth: number): number {
    let width = 0;
    for (const ch of text) {
        width += ch === '\t' ? tabWidth : 1;
    }
    return width;
}
