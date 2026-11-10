/**
 * The literal arguments control calls must have. The control panel shows a
 * buffer's controls before its patch is evaluated, reading them from the
 * source, and rewrites their literals in place; a control whose arguments
 * are not literals could not be shown or edited, so evaluation rejects it.
 */
import ts from 'typescript';

import { booleanLiteral, negatableNumber } from '../../shared/dsl/ast/literals';
import { propertyName } from '../../shared/dsl/ast/names';
import { lineColumn } from '../../shared/dsl/ast/position';
import type { ControlKind } from '../../shared/dsl/controlCalls';

function at(node: ts.Node, file: ts.SourceFile): string {
    const { line, column } = lineColumn(file, node.getStart(file));
    return `at line ${line}, column ${column}`;
}

/** The control panel keys widgets by label, so it must be statically known. */
function requireStringLiteralLabel(
    kind: ControlKind,
    args: readonly ts.Expression[],
    file: ts.SourceFile,
): void {
    if (args.length >= 1 && !ts.isStringLiteral(args[0])) {
        throw new Error(
            `${kind}() label (argument 1) must be a string literal ${at(args[0], file)}`,
        );
    }
}

function isSliderLiteral(node: ts.Node): boolean {
    return ts.isStringLiteral(node) || negatableNumber(node) !== null;
}

/** Throw unless the arguments of control call `call` are literals the
 *  control panel can read and rewrite. */
export function validateControlCall(
    call: ts.CallExpression,
    kind: ControlKind,
    file: ts.SourceFile,
): void {
    const args = call.arguments;
    requireStringLiteralLabel(kind, args, file);
    if (kind === '$slider') {
        const names = ['value', 'min', 'max'];
        for (let i = 1; i < Math.min(args.length, 4); i++) {
            if (!isSliderLiteral(args[i])) {
                throw new Error(
                    `$slider() ${names[i - 1]} (argument ${i + 1}) must be a numeric or string literal ${at(args[i], file)}`,
                );
            }
        }
    } else if (kind === '$toggleBtn') {
        if (args.length >= 2 && booleanLiteral(args[1]) === null) {
            throw new Error(
                `$toggleBtn() initial state (argument 2) must be a true or false literal ${at(args[1], file)}`,
            );
        }
    } else if (kind === '$cGroup') {
        // The control scan only reports a group whose params are absent or
        // an object literal; read `collapsed` as it does, so a quoted key
        // counts.
        const params = args[1];
        const collapsed =
            params && ts.isObjectLiteralExpression(params)
                ? params.properties.find((p) => propertyName(p) === 'collapsed')
                : undefined;
        const value =
            collapsed && ts.isPropertyAssignment(collapsed)
                ? booleanLiteral(collapsed.initializer)
                : null;
        if (collapsed && value === null) {
            throw new Error(
                `$cGroup() collapsed must be a true or false literal ${at(collapsed, file)}`,
            );
        }
    }
}
