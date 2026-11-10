/**
 * Utility for editing the `mute` / `solo` / `pan` / `gain` options of
 * `.out(...)` and `.outMono(...)` calls, and the `$setOutputGain(...)` level,
 * in DSL source code.
 *
 * The VU meter panel's buttons anchor each out call with a Monaco tracked
 * decoration; this module turns that anchor (a character offset at the method
 * name) plus the desired property change into a single text edit, reading
 * the call, its arguments, its options object and their separating commas
 * from the syntax tree.
 */
import ts from 'typescript';

import { booleanLiteral, negatableNumber } from '../../shared/dsl/ast/literals';
import { propertyName } from '../../shared/dsl/ast/names';
import { parseDsl } from '../../shared/dsl/ast/parse';
import { findDescendant, forEachDescendant } from '../../shared/dsl/ast/walk';
import { callParens, closingTokenStart } from './listEdits';

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

/** The `.out` / `.outMono` call whose method name starts at `anchor`. */
function outCallAt(
    file: ts.SourceFile,
    anchor: number,
): ts.CallExpression | undefined {
    return findDescendant(
        file,
        (node): node is ts.CallExpression =>
            ts.isCallExpression(node) &&
            ts.isPropertyAccessExpression(node.expression) &&
            (node.expression.name.text === 'out' ||
                node.expression.name.text === 'outMono') &&
            node.expression.name.getStart(file) === anchor,
        anchor,
    );
}

/** The comma tokens on either side of `element` in the argument or
 *  property list holding it. */
function commasAround(
    element: ts.Node,
    file: ts.SourceFile,
): { before?: ts.Node; after?: ts.Node } {
    const list = element.parent
        .getChildren(file)
        .find(
            (c) =>
                c.kind === ts.SyntaxKind.SyntaxList &&
                c.pos <= element.pos &&
                element.end <= c.end,
        );
    const items = list?.getChildren(file) ?? [];
    const i = items.indexOf(element);
    const comma = (n: ts.Node | undefined) =>
        n?.kind === ts.SyntaxKind.CommaToken ? n : undefined;
    return {
        after: i < 0 ? undefined : comma(items[i + 1]),
        before: i < 0 ? undefined : comma(items[i - 1]),
    };
}

/**
 * The span that removes `element` from its list along with one separator.
 * An element on lines of its own goes with those whole lines, its trailing
 * comment included, so its neighbours keep their comments and layout. An
 * inline element goes with the comma after it, or else the one before it.
 */
function removalSpan(
    element: ts.Node,
    file: ts.SourceFile,
): { start: number; end: number } {
    const text = file.text;
    const { before, after } = commasAround(element, file);
    const start = element.getStart(file);
    const tail = after ? after.getEnd() : element.getEnd();
    const lineStart = text.lastIndexOf('\n', start - 1) + 1;
    const lineEnd = text.indexOf('\n', tail);
    if (
        lineEnd >= 0 &&
        /^\s*$/.test(text.slice(lineStart, start)) &&
        /^\s*(\/\/[^\n]*)?$/.test(text.slice(tail, lineEnd))
    ) {
        return { end: lineEnd + 1, start: lineStart };
    }
    if (after) {
        let end = after.getEnd();
        while (text[end] === ' ' || text[end] === '\t') {
            end++;
        }
        return { end, start };
    }
    return {
        end: element.getEnd(),
        start: before ? before.getStart(file) : start,
    };
}

/** Whether `node` sits inside a function, where it runs only if called. */
function insideFunction(node: ts.Node): boolean {
    for (let n = node.parent; n; n = n.parent) {
        if (ts.isFunctionLike(n)) {
            return true;
        }
    }
    return false;
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
 * numeric literal in the last `$setOutputGain(...)` outside any function,
 * the one whose level the patch ends up with, or appends a new call at the
 * end of the source. Returns null when that call's argument is not a
 * numeric literal (signal-driven — the fader is locked), or when it is not a
 * statement of its own at the top of the patch, so whether it runs depends
 * on a branch or loop.
 */
export function computeSetOutputGainEdit(
    source: string,
    value: number,
): OutOptionEdit | null {
    const newText = String(Number(value.toPrecision(4)));
    const file = parseDsl(source);
    let last: ts.CallExpression | undefined;
    forEachDescendant(file, (node) => {
        if (
            ts.isCallExpression(node) &&
            ts.isIdentifier(node.expression) &&
            node.expression.text === '$setOutputGain' &&
            !insideFunction(node) &&
            (!last || node.getStart(file) > last.getStart(file))
        ) {
            last = node;
        }
    });
    if (last) {
        const arg = last.arguments[0];
        if (
            !ts.isExpressionStatement(last.parent) ||
            last.parent.parent !== file ||
            !callParens(last, file) ||
            last.arguments.length !== 1 ||
            !isNumber(arg)
        ) {
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
    const call = outCallAt(file, anchorOffset);
    const bounds = call && callParens(call, file);
    if (!call || !bounds) {
        return null;
    }
    const method = (call.expression as ts.PropertyAccessExpression).name.text;
    const { open, close } = bounds;
    const args = call.arguments;

    // `.out(...)` carries its options object as the sole argument. `.outMono`
    // normally takes a leading positional channel, so its options object is
    // the second argument — unless the single-object overload `.outMono({…})`
    // placed it first (an object as the first argument).
    const firstIsObject =
        args[0] !== undefined && ts.isObjectLiteralExpression(args[0]);
    const optionsIndex = method === 'out' || firstIsObject ? 0 : 1;
    const optionsArg = args[optionsIndex] as ts.Expression | undefined;
    // Arguments past the options are no form these edits know.
    const optionsLast = args.length === optionsIndex + 1;
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
                if (ts.isSpreadElement(optionsArg)) {
                    return null;
                }
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

        const objClose = closingTokenStart(
            options,
            file,
            ts.SyntaxKind.CloseBraceToken,
        );
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
        const last = options.properties.at(-1);
        if (!last) {
            return {
                end: objClose + 1,
                start: options.getStart(file),
                text: `{ ${prop}: ${newText} }`,
            };
        }
        // After the last property's own comma, or after the property with a
        // comma of its own; either way ahead of any comment on that line.
        const trailing = commasAround(last, file).after;
        const at = trailing ? trailing.getEnd() : last.getEnd();
        return {
            end: at,
            start: at,
            text: `${trailing ? '' : ','} ${prop}: ${newText}`,
        };
    }

    // --- Remove the property ---
    if (!options) {
        // A positional outMono gain literal is removable in place.
        if (
            method === 'outMono' &&
            prop === 'gain' &&
            optionsArg !== undefined &&
            optionsLast &&
            isLiteral(optionsArg)
        ) {
            return { ...removalSpan(optionsArg, file), text: '' };
        }
        return null;
    }
    const objClose = closingTokenStart(
        options,
        file,
        ts.SyntaxKind.CloseBraceToken,
    );
    const existing = objClose === null ? null : findProperty(options, prop);
    if (
        objClose === null ||
        !existing ||
        !ts.isPropertyAssignment(existing) ||
        !isLiteral(existing.initializer)
    ) {
        return null;
    }

    // Removing one of a repeated key would leave an earlier one in force.
    if (
        options.properties.some(
            (p) => p !== existing && propertyName(p) === prop,
        )
    ) {
        return null;
    }
    const span = removalSpan(existing, file);
    const leftover =
        source.slice(options.getStart(file) + 1, span.start) +
        source.slice(span.end, objClose);
    // The only property, with no comment beside it to keep: drop the whole
    // options argument.
    if (options.properties.length === 1 && leftover.trim() === '') {
        if (!optionsLast) {
            return null;
        }
        if (method === 'out') {
            return { end: close, start: open, text: '' };
        }
        return { ...removalSpan(options, file), text: '' };
    }
    return { ...span, text: '' };
}

/** The member of `obj` that sets `prop`. */
function findProperty(
    obj: ts.ObjectLiteralExpression,
    prop: string,
): ts.ObjectLiteralElementLike | null {
    // The last of a repeated key is the one the object keeps.
    return obj.properties.findLast((p) => propertyName(p) === prop) ?? null;
}
