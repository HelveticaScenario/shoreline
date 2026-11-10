import ts from 'typescript';

/**
 * A numeric literal, bare or under a unary `-` or `+`. Each caller decides
 * which signs it accepts: control arguments take only `-`, as evaluation
 * does.
 */
export function signedNumericLiteral(
    node: ts.Node,
): { sign: '-' | '+' | null; literal: ts.NumericLiteral } | null {
    if (ts.isNumericLiteral(node)) {
        return { literal: node, sign: null };
    }
    if (
        ts.isPrefixUnaryExpression(node) &&
        (node.operator === ts.SyntaxKind.MinusToken ||
            node.operator === ts.SyntaxKind.PlusToken) &&
        ts.isNumericLiteral(node.operand)
    ) {
        return {
            literal: node.operand,
            sign: node.operator === ts.SyntaxKind.MinusToken ? '-' : '+',
        };
    }
    return null;
}

/** The value of a numeric literal, bare or negated, or null: the numbers a
 *  control argument may be written as. */
export function negatableNumber(node: ts.Node): number | null {
    const number = signedNumericLiteral(node);
    if (!number || number.sign === '+') {
        return null;
    }
    const value = Number(number.literal.text);
    return number.sign === '-' ? -value : value;
}

/** The value of a `true` / `false` literal, or null for any other node. */
export function booleanLiteral(node: ts.Node): boolean | null {
    if (node.kind === ts.SyntaxKind.TrueKeyword) {
        return true;
    }
    if (node.kind === ts.SyntaxKind.FalseKeyword) {
        return false;
    }
    return null;
}
