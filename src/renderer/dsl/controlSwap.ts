/**
 * Swaps between a `$c`/`$signal` holding one literal and a `$slider`: the
 * checks that a swap keeps the patch's behaviour, and the fix turning a
 * slider back into a `$c`.
 */
import ts from 'typescript';

import type { ControlQuickFix } from './controlQuickFix';
import { asLiteral } from './paramCallResolver';
import type { ParamLiteral } from './paramControls';

/** Calls that only lift a value into a signal; a control can replace one
 *  holding a single literal outright. */
const SIGNAL_LIFTS = new Set(['$c', '$signal']);

/** The callees a swap between a lift and a slider leaves in the code. */
export const LIFT_TO_SLIDER = ['$c', '$signal', '$slider'];

/**
 * Whether `call` can be swapped between a `$c`/`$signal` and a `$slider`
 * without changing what the patch does. Neither swap holds where the patch
 * declares its own binding of one of `callees`, or where the call's value —
 * the call itself, or the variable it initializes — has `.range` called on
 * it: a slider's `.range(outMin, outMax)` maps from its own min and max,
 * where a `$c`'s takes the input range as well.
 */
export function createSwapCheck(
    file: ts.SourceFile,
): (call: ts.CallExpression, callees: readonly string[]) => boolean {
    const declared = new Set<string>();
    const ranged = new Set<string>();
    const visit = (node: ts.Node) => {
        if (
            (ts.isVariableDeclaration(node) ||
                ts.isParameter(node) ||
                ts.isBindingElement(node) ||
                ts.isFunctionDeclaration(node)) &&
            node.name &&
            ts.isIdentifier(node.name)
        ) {
            declared.add(node.name.text);
        }
        if (
            ts.isPropertyAccessExpression(node) &&
            node.name.text === 'range' &&
            ts.isIdentifier(node.expression)
        ) {
            ranged.add(node.expression.text);
        }
        ts.forEachChild(node, visit);
    };
    visit(file);
    return (call, callees) => {
        if (callees.some((name) => declared.has(name))) {
            return false;
        }
        const parent = call.parent;
        if (
            ts.isPropertyAccessExpression(parent) &&
            parent.expression === call &&
            parent.name.text === 'range'
        ) {
            return false;
        }
        return !(
            ts.isVariableDeclaration(parent) &&
            ts.isIdentifier(parent.name) &&
            ranged.has(parent.name.text)
        );
    };
}

/** `node` as a `$c`/`$signal` call holding just one literal. */
export function singleLiteralSignal(
    node: ts.Node,
    file: ts.SourceFile,
): { call: ts.CallExpression; literal: ParamLiteral } | null {
    if (
        !ts.isCallExpression(node) ||
        !ts.isIdentifier(node.expression) ||
        !SIGNAL_LIFTS.has(node.expression.text) ||
        node.arguments.length !== 1
    ) {
        return null;
    }
    const literal = asLiteral(node.arguments[0], file);
    return literal ? { call: node, literal } : null;
}

/**
 * "Replace $slider with $c(…)": the innermost `$slider` call at the cursor
 * becomes a `$c` of its value, as written.
 */
export function unwrapFixes(
    file: ts.SourceFile,
    atCursor: readonly ts.Node[],
    containsCursor: (n: ts.Node) => boolean,
    swappable: (call: ts.CallExpression, callees: readonly string[]) => boolean,
): ControlQuickFix[] {
    const slider = atCursor.find(
        (n): n is ts.CallExpression =>
            ts.isCallExpression(n) &&
            ts.isIdentifier(n.expression) &&
            n.expression.text === '$slider' &&
            containsCursor(n),
    );
    const value = slider?.arguments[1];
    if (
        !slider ||
        !value ||
        !asLiteral(value, file) ||
        !swappable(slider, LIFT_TO_SLIDER)
    ) {
        return [];
    }
    const newText = `$c(${value.getText(file)})`;
    return [
        {
            caretOffset: newText.length,
            controlOffset: 0,
            kind: 'unwrap',
            newText,
            span: { end: slider.getEnd(), start: slider.getStart(file) },
            title: `Replace $slider with ${newText}`,
        },
    ];
}

/** A single-literal `$c`/`$signal` call as a title shows it: callee and
 *  literal, without the call's spacing or comments. */
export function liftText(call: ts.CallExpression, file: ts.SourceFile): string {
    const literal = call.arguments[0].getText(file);
    return `${call.expression.getText(file)}(${literal.length <= 16 ? literal : '…'})`;
}
