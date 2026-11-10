/**
 * How template literals interpolate the patch's `const` bindings. When
 * `${root} e4 g4` interpolates a const string, the resolution records where
 * the const's value lands in the evaluated text and the span of the const's
 * own literal, so a highlight inside the evaluated pattern can be redirected
 * to the declaration. Consts whose initializer is itself a template resolve
 * recursively. A const number only advances the offsets: its source text is
 * not the text it prints, so nothing inside it can be redirected.
 */
import ts from 'typescript';

import { constInitializerOf } from '../../shared/dsl/ast/consts';
import { signedNumericLiteral } from '../../shared/dsl/ast/literals';
import type {
    ResolvedInterpolation,
    SourceSpan,
} from '../../shared/dsl/spanTypes';

/** String, numeric (signed or not), template, array and object literals:
 *  the argument forms whose spans are recorded. */
export function isTrackableLiteral(node: ts.Node): boolean {
    return (
        ts.isStringLiteral(node) ||
        signedNumericLiteral(node) !== null ||
        ts.isNoSubstitutionTemplateLiteral(node) ||
        ts.isTemplateExpression(node) ||
        ts.isArrayLiteralExpression(node) ||
        ts.isObjectLiteralExpression(node)
    );
}

export function spanOf(node: ts.Node, file: ts.SourceFile): SourceSpan {
    return { end: node.getEnd(), start: node.getStart(file) };
}

/** The literal the const an identifier refers to holds, or null when it
 *  refers to no const, or to one whose value is not a trackable literal. */
export function constLiteralOf(id: ts.Identifier): ts.Expression | null {
    const init = constInitializerOf(id);
    return init && isTrackableLiteral(init) ? init : null;
}

/** Length of the string a literal evaluates to, or null when the source
 *  alone cannot tell. */
function evaluatedLength(
    node: ts.Node,
    file: ts.SourceFile,
    visited = new Set<string>(),
): number | null {
    if (ts.isStringLiteral(node) || ts.isNoSubstitutionTemplateLiteral(node)) {
        return node.text.length;
    }
    const number = signedNumericLiteral(node);
    if (number) {
        // The number as a template prints it: `0x10` interpolates as `16`.
        const value = Number(number.literal.text);
        return String(number.sign === '-' ? -value : value).length;
    }
    if (!ts.isTemplateExpression(node)) {
        return null;
    }
    let total = node.head.text.length;
    for (const span of node.templateSpans) {
        const length = interpolatedLength(span.expression, file, visited);
        if (length === null) {
            return null;
        }
        total += length + span.literal.text.length;
    }
    return total;
}

/** Length of what an interpolated expression evaluates to: a const literal
 *  (guarding circular references), or a literal written in place. */
function interpolatedLength(
    expr: ts.Expression,
    file: ts.SourceFile,
    visited: Set<string>,
): number | null {
    if (ts.isIdentifier(expr)) {
        const literal = constLiteralOf(expr);
        if (visited.has(expr.text) || !literal) {
            return null;
        }
        visited.add(expr.text);
        const length = evaluatedLength(literal, file, visited);
        visited.delete(expr.text);
        return length;
    }
    return evaluatedLength(expr, file, visited);
}

/**
 * The const interpolations of a template expression, each with where its
 * value starts in the evaluated text. Resolution stops at the first
 * interpolation whose length is unknown, since every later offset would be
 * wrong; the ones before it are kept. Null when nothing resolves.
 */
export function resolveInterpolations(
    node: ts.Node,
    file: ts.SourceFile,
): ResolvedInterpolation[] | null {
    if (!ts.isTemplateExpression(node)) {
        return null;
    }
    const resolutions: ResolvedInterpolation[] = [];
    const settled = () => (resolutions.length > 0 ? resolutions : null);
    let offset = node.head.text.length;
    for (const span of node.templateSpans) {
        const expr = span.expression;
        if (ts.isIdentifier(expr)) {
            const literal = constLiteralOf(expr);
            const length = literal ? evaluatedLength(literal, file) : null;
            if (!literal || length === null) {
                return settled();
            }
            if (!signedNumericLiteral(literal)) {
                resolutions.push({
                    constLiteralSpan: spanOf(literal, file),
                    evaluatedLength: length,
                    evaluatedStart: offset,
                    nestedResolutions:
                        resolveInterpolations(literal, file) ?? undefined,
                });
            }
            offset += length;
        } else {
            const length = interpolatedLength(expr, file, new Set());
            if (length === null) {
                return settled();
            }
            offset += length;
        }
        offset += span.literal.text.length;
    }
    return settled();
}
