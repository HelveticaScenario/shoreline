/**
 * Positions in patch source as V8 reports them for the running patch, so a
 * node found in the tree can be matched to the call site a factory captures
 * with `captureSourceLocation()`.
 */
import ts from 'typescript';

import { FIRST_LINE_COLUMN_OFFSET } from '../spanTypes';

/** 1-based line and column of `pos`. Line breaks are the ones V8 counts:
 *  LF, CR, CRLF, U+2028 and U+2029. */
export function lineColumn(
    file: ts.SourceFile,
    pos: number,
): { line: number; column: number } {
    const { line, character } = file.getLineAndCharacterOfPosition(pos);
    return { column: character + 1, line: line + 1 };
}

/** Where V8 places a call: at the method name for `a.b(...)`, at the start
 *  of the call otherwise. */
export function v8CallAnchor(
    call: ts.CallExpression,
    file: ts.SourceFile,
): number {
    const callee = call.expression;
    return ts.isPropertyAccessExpression(callee)
        ? callee.name.getStart(file)
        : call.getStart(file);
}

/**
 * The `line:column` V8 reports for code at `pos`: the executor runs line 1 of
 * the patch indented by FIRST_LINE_COLUMN_OFFSET, and `lineOffset` adds the
 * script wrapper's lines before it.
 */
export function callSiteKey(
    file: ts.SourceFile,
    pos: number,
    lineOffset = 0,
): `${number}:${number}` {
    const { line, column } = lineColumn(file, pos);
    const shift = line === 1 ? FIRST_LINE_COLUMN_OFFSET : 0;
    return `${line + lineOffset}:${column + shift}`;
}
