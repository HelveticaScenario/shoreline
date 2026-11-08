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
