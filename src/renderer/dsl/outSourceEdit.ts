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

/** Start of `element` together with the comma separating it from the one
 *  before, so removing from there drops it and its separator. */
function startWithComma(element: ts.Node, file: ts.SourceFile): number {
    const { before } = commasAround(element, file);
    return before ? before.getStart(file) : element.getStart(file);
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
 * numeric literal in the last `$setOutputGain(...)` call, the one whose
 * level the patch ends up with, or appends a new call at the end of the
 * source. Returns null when that call's argument is not a numeric literal
 * (signal-driven — the fader is locked).
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
            (!last || node.getStart(file) > last.getStart(file))
        ) {
            last = node;
        }
    });
    if (last) {
        const arg = last.arguments[0];
        if (
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
            isLiteral(optionsArg)
        ) {
            return {
                end: optionsArg.getEnd(),
                start: startWithComma(optionsArg, file),
                text: '',
            };
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

    // If this was the only property, drop the whole options argument.
    if (options.properties.length === 1) {
        if (method === 'out') {
            return { end: close, start: open, text: '' };
        }
        // `.outMono(ch, { prop: true })` — remove the second arg and the
        // comma separating it from the channel.
        return {
            end: options.getEnd(),
            start: startWithComma(options, file),
            text: '',
        };
    }

    // Through the property's own comma and the space after it, or else
    // back through the comma before it.
    const { after } = commasAround(existing, file);
    if (after) {
        let end = after.getEnd();
        while (end < objClose && /\s/.test(source[end])) {
            end++;
        }
        return { end, start: existing.getStart(file), text: '' };
    }
    return {
        end: existing.initializer.getEnd(),
        start: startWithComma(existing, file),
        text: '',
    };
}

/** The member of `obj` that sets `prop`. */
function findProperty(
    obj: ts.ObjectLiteralExpression,
    prop: string,
): ts.ObjectLiteralElementLike | null {
    return obj.properties.find((p) => propertyName(p) === prop) ?? null;
}
