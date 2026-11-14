/**
 * Names read from the syntax tree rather than the text, so whitespace,
 * newlines, and comments inside an expression never reach them.
 */
import ts from 'typescript';

/**
 * `foo` for an identifier, `a.b.c` for a property-access chain rooted at an
 * identifier, and null for anything else.
 */
export function dottedName(expr: ts.Expression): string | null {
    if (ts.isIdentifier(expr)) {
        return expr.text;
    }
    if (ts.isPropertyAccessExpression(expr)) {
        const base = dottedName(expr.expression);
        return base === null ? null : `${base}.${expr.name.text}`;
    }
    return null;
}

/** A property's static key, or null for a computed or spread member. */
export function propertyName(p: ts.ObjectLiteralElementLike): string | null {
    if (ts.isSpreadAssignment(p) || !p.name) {
        return null;
    }
    return ts.isIdentifier(p.name) ||
        ts.isStringLiteral(p.name) ||
        ts.isNumericLiteral(p.name)
        ? p.name.text
        : null;
}

/** A `` $`source` `` pattern literal. */
export function isPatternTag(node: ts.TaggedTemplateExpression): boolean {
    return ts.isIdentifier(node.tag) && node.tag.text === '$';
}
