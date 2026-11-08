/**
 * Utility for editing the `mute` / `solo` / `pan` / `gain` options of
 * `.out(...)` and `.outMono(...)` calls, and the `$setOutputGain(...)` level,
 * in DSL source code.
 *
 * The VU meter panel's buttons anchor each out call with a Monaco tracked
 * decoration; this module turns that anchor (a character offset at the method
 * name) plus the desired property change into a single text edit, reading
 * the call, its arguments and its options object from the syntax tree.
 */
import ts from 'typescript';

import { booleanLiteral, negatableNumber } from '../../shared/dsl/ast/literals';
import { propertyName } from '../../shared/dsl/ast/names';
import { parseDsl } from '../../shared/dsl/ast/parse';
import { forEachDescendant } from '../../shared/dsl/ast/walk';

export type OutOptionProp = 'mute' | 'solo';

export interface OutOptionEdit {
    /** Inclusive start character offset */
    start: number;
    /** Exclusive end character offset */
    end: number;
    /** Replacement text */
    text: string;
}

/** Whether `node` is a literal an option's value may be rewritten from. */
type LiteralCheck = (node: ts.Node) => boolean;

const isBoolean: LiteralCheck = (node) => booleanLiteral(node) !== null;
const isNumber: LiteralCheck = (node) => negatableNumber(node) !== null;

/** The call's parentheses: just past `(`, and at `)`. Null when the parser
 *  found no closing paren. */
function parens(
    call: ts.CallExpression,
    file: ts.SourceFile,
): { open: number; close: number } | null {
    const children = call.getChildren(file);
    const open = children.find((c) => c.kind === ts.SyntaxKind.OpenParenToken);
    const close = children.findLast(
        (c) => c.kind === ts.SyntaxKind.CloseParenToken,
    );
    return open && close
        ? { close: close.getStart(file), open: open.getEnd() }
        : null;
}

/** The `.out` / `.outMono` call whose method name starts at `anchor`. */
function outCallAt(
    file: ts.SourceFile,
    anchor: number,
): { call: ts.CallExpression; method: 'out' | 'outMono' } | null {
    let found: { call: ts.CallExpression; method: 'out' | 'outMono' } | null =
        null;
    forEachDescendant(file, (node) => {
        if (found || !ts.isCallExpression(node)) {
            return;
        }
        const callee = node.expression;
        if (
            ts.isPropertyAccessExpression(callee) &&
            (callee.name.text === 'out' || callee.name.text === 'outMono') &&
            callee.name.getStart(file) === anchor
        ) {
            found = { call: node, method: callee.name.text };
        }
    });
    return found;
}

/** An object literal's closing brace, or null for one the parser closed
 *  without it. */
function closeBrace(
    obj: ts.ObjectLiteralExpression,
    file: ts.SourceFile,
): number | null {
    const last = obj.getLastToken(file);
    return last?.kind === ts.SyntaxKind.CloseBraceToken
        ? last.getStart(file)
        : null;
}

/** Start of `node` widened back over whitespace and one comma before it,
 *  so removing from there drops the argument and its separator. */
function withLeadingComma(
    source: string,
    node: ts.Node,
    file: ts.SourceFile,
    floor: number,
): number {
    let start = node.getStart(file);
    while (start > floor && /\s/.test(source[start - 1])) {
        start--;
    }
    return source[start - 1] === ',' ? start - 1 : start;
}

/**
 * Compute the text edit that sets (`value` true) or removes (`value` false)
 * the `prop` property in the options argument of the `.out(...)` /
 * `.outMono(...)` call whose method name starts at `anchorOffset`.
 *
 * Returns null when the call can't be found or the existing property value
 * is not a `true`/`false` literal — callers skip the source edit but still
 * apply the live graph update (the same silent-skip contract as sliders).
 */
export function computeOutOptionEdit(
    source: string,
    anchorOffset: number,
    prop: OutOptionProp,
    value: boolean,
): OutOptionEdit | null {
    return computeOptionPropertyEdit(
        source,
        anchorOffset,
        prop,
        value ? 'true' : null,
        isBoolean,
    );
}

/**
 * Same contract as `computeOutOptionEdit` for numeric-valued options (the
 * pan knob): a number sets `prop: <value>`, null removes the property. An
 * existing value must be a numeric literal or the edit is skipped.
 */
export function computeOutNumericOptionEdit(
    source: string,
    anchorOffset: number,
    prop: string,
    value: number | null,
): OutOptionEdit | null {
    return computeOptionPropertyEdit(
        source,
        anchorOffset,
        prop,
        value === null ? null : String(Number(value.toPrecision(4))),
        isNumber,
    );
}

/**
 * Compute the text edit that sets the master output gain: updates the
 * numeric literal in the first `$setOutputGain(...)` call, or appends a new
 * call at the end of the source. Returns null when that call's argument is
 * not a numeric literal (signal-driven — the fader is locked).
 */
export function computeSetOutputGainEdit(
    source: string,
    value: number,
): OutOptionEdit | null {
    const newText = String(Number(value.toPrecision(4)));
    const file = parseDsl(source);
    let call: ts.CallExpression | null = null;
    forEachDescendant(file, (node) => {
        if (
            !call &&
            ts.isCallExpression(node) &&
            ts.isIdentifier(node.expression) &&
            node.expression.text === '$setOutputGain'
        ) {
            call = node;
        }
    });
    if (call) {
        const { arguments: args } = call as ts.CallExpression;
        const arg = args[0];
        if (!parens(call, file) || args.length !== 1 || !isNumber(arg)) {
            return null;
        }
        return { end: arg.getEnd(), start: arg.getStart(file), text: newText };
    }
    // No call yet — append one on its own line at the end.
    const needsNewline = source.length > 0 && !source.endsWith('\n');
    return {
        end: source.length,
        start: source.length,
        text: `${needsNewline ? '\n' : ''}$setOutputGain(${newText})\n`,
    };
}

function computeOptionPropertyEdit(
    source: string,
    anchorOffset: number,
    prop: string,
    newText: string | null,
    isLiteral: LiteralCheck,
): OutOptionEdit | null {
    const file = parseDsl(source);
    const found = outCallAt(file, anchorOffset);
    const bounds = found && parens(found.call, file);
    if (!found || !bounds) {
        return null;
    }
    const { call, method } = found;
    const { open, close } = bounds;
    const args = call.arguments;

    // `.out(...)` carries its options object as the sole argument. `.outMono`
    // normally takes a leading positional channel, so its options object is
    // the second argument — unless the single-object overload `.outMono({…})`
    // placed it first (an object as the first argument).
    const firstIsObject =
        args[0] !== undefined && ts.isObjectLiteralExpression(args[0]);
    const optionsArg = args[method === 'out' || firstIsObject ? 0 : 1] as
        | ts.Expression
        | undefined;
    const options =
        optionsArg && ts.isObjectLiteralExpression(optionsArg)
            ? optionsArg
            : null;

    if (newText !== null) {
        // --- Set `prop: <newText>` ---
        if (!options) {
            if (method === 'out') {
                // `.out()` — arg list must be empty (a non-object first arg
                // is not a valid out() call).
                if (optionsArg !== undefined) {
                    return null;
                }
                return {
                    end: close,
                    start: open,
                    text: `{ ${prop}: ${newText} }`,
                };
            }
            // `.outMono(...)` — wrap a positional gain, append an options
            // object, or supply the default channel for a bare call. Setting
            // `gain` itself keeps the positional form.
            if (optionsArg !== undefined) {
                const span = {
                    end: optionsArg.getEnd(),
                    start: optionsArg.getStart(file),
                };
                if (prop === 'gain') {
                    return isLiteral(optionsArg)
                        ? { ...span, text: newText }
                        : null;
                }
                const gainText = optionsArg.getText(file);
                return {
                    ...span,
                    text: `{ gain: ${gainText}, ${prop}: ${newText} }`,
                };
            }
            if (args.length === 1) {
                const end = args[0].getEnd();
                return {
                    end,
                    start: end,
                    text:
                        prop === 'gain'
                            ? `, ${newText}`
                            : `, { ${prop}: ${newText} }`,
                };
            }
            return {
                end: close,
                start: open,
                text:
                    prop === 'gain'
                        ? `0, ${newText}`
                        : `0, { ${prop}: ${newText} }`,
            };
        }

        const objClose = closeBrace(options, file);
        if (objClose === null) {
            return null;
        }
        const existing = findProperty(options, prop);
        if (existing) {
            if (
                !ts.isPropertyAssignment(existing) ||
                !isLiteral(existing.initializer)
            ) {
                return null;
            }
            const init = existing.initializer;
            return {
                end: init.getEnd(),
                start: init.getStart(file),
                text: newText,
            };
        }
        const objOpen = options.getStart(file);
        if (options.properties.length === 0) {
            return {
                end: objClose + 1,
                start: objOpen,
                text: `{ ${prop}: ${newText} }`,
            };
        }
        // Insert before the closing brace, after any trailing comma.
        let insertAt = objClose;
        while (insertAt > objOpen + 1 && /\s/.test(source[insertAt - 1])) {
            insertAt--;
        }
        const needsComma = source[insertAt - 1] !== ',';
        return {
            end: objClose,
            start: insertAt,
            text: `${needsComma ? ',' : ''} ${prop}: ${newText} `,
        };
    }

    // --- Remove the property ---
    if (!options) {
        // A positional outMono gain literal is removable in place.
        if (
            method === 'outMono' &&
            prop === 'gain' &&
            optionsArg !== undefined &&
            isLiteral(optionsArg)
        ) {
            return {
                end: optionsArg.getEnd(),
                start: withLeadingComma(source, optionsArg, file, open),
                text: '',
            };
        }
        return null;
    }
    const objClose = closeBrace(options, file);
    const existing = objClose === null ? null : findProperty(options, prop);
    if (
        objClose === null ||
        !existing ||
        !ts.isPropertyAssignment(existing) ||
        !isLiteral(existing.initializer)
    ) {
        return null;
    }
    const objOpen = options.getStart(file);

    // Property removal span: through the following comma if present,
    // otherwise back through the preceding comma.
    let removeStart = existing.getStart(file);
    let removeEnd = existing.initializer.getEnd();
    let k = removeEnd;
    while (k < objClose && /\s/.test(source[k])) {
        k++;
    }
    if (source[k] === ',') {
        removeEnd = k + 1;
        while (removeEnd < objClose && /\s/.test(source[removeEnd])) {
            removeEnd++;
        }
    } else {
        removeStart = withLeadingComma(source, existing, file, objOpen + 1);
    }

    // If this was the only property, drop the whole options argument.
    const remainder =
        source.slice(objOpen + 1, removeStart) +
        source.slice(removeEnd, objClose);
    if (/^\s*$/.test(remainder)) {
        if (method === 'out') {
            return { end: close, start: open, text: '' };
        }
        // `.outMono(ch, { prop: true })` — remove the second arg and the
        // comma separating it from the channel.
        return {
            end: options.getEnd(),
            start: withLeadingComma(source, options, file, open),
            text: '',
        };
    }
    return { end: removeEnd, start: removeStart, text: '' };
}

/** The member of `obj` that sets `prop`. */
function findProperty(
    obj: ts.ObjectLiteralExpression,
    prop: string,
): ts.ObjectLiteralElementLike | null {
    return obj.properties.find((p) => propertyName(p) === prop) ?? null;
}
