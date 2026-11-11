/**
 * Swaps between a `$c`/`$signal` holding one literal and a `$slider`, and
 * the fix turning a slider back into a `$c`. A swap is a plain substitution:
 * a slider's `.range(outMin, outMax)` maps from its own min and max where a
 * `$c`'s takes the input range too, and calls the swap leaves behind are
 * the user's to adjust.
 */
import ts from 'typescript';

import type { ControlQuickFix } from './controlQuickFix';
import { asLiteral } from './paramCallResolver';
import type { ParamLiteral } from './paramControls';

/** Calls that only lift a value into a signal; a control can replace one
 *  holding a single literal outright. */
const SIGNAL_LIFTS = new Set(['$c', '$signal']);

/**
 * Whether the patch declares its own `$c`, `$signal` or `$slider`, so a swap
 * between them would replace or call the patch's binding.
 */
export function declaresSwapNames(file: ts.SourceFile): boolean {
    const names = new Set(['$c', '$signal', '$slider']);
    let found = false;
    const visit = (node: ts.Node) => {
        if (
            (ts.isVariableDeclaration(node) ||
                ts.isParameter(node) ||
                ts.isBindingElement(node) ||
                ts.isFunctionDeclaration(node)) &&
            node.name &&
            ts.isIdentifier(node.name) &&
            names.has(node.name.text)
        ) {
            found = true;
        }
        if (!found) {
            ts.forEachChild(node, visit);
        }
    };
    visit(file);
    return found;
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
): ControlQuickFix[] {
    const slider = atCursor.find(
        (n): n is ts.CallExpression =>
            ts.isCallExpression(n) &&
            ts.isIdentifier(n.expression) &&
            n.expression.text === '$slider' &&
            containsCursor(n),
    );
    const value = slider?.arguments[1];
    if (!slider || !value || !asLiteral(value, file)) {
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
