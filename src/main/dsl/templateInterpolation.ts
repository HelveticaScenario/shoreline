/**
 * The patch's top-level `const` bindings, and how template literals
 * interpolate them. When `${root} e4 g4` interpolates a const string, the
 * resolution records where the const's value lands in the evaluated text and
 * the span of the const's own literal, so a highlight inside the evaluated
 * pattern can be redirected to the declaration. Consts whose initializer is
 * itself a template resolve recursively.
 */
import ts from 'typescript';

import { signedNumericLiteral } from '../../shared/dsl/ast/literals';
import type {
    ResolvedInterpolation,
    SourceSpan,
} from '../../shared/dsl/spanTypes';

/** Top-level `const` initializers by name; a later declaration wins. */
export interface ConstTable {
    file: ts.SourceFile;
    /** Every initializer. */
    initializers: Map<string, ts.Expression>;
    /** The latest initializer of each name that was a trackable literal. */
    literals: Map<string, ts.Expression>;
}

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

/** `const` declarations at the top level only, which is where a flat patch
 *  script declares them. */
export function buildConstTable(file: ts.SourceFile): ConstTable {
    const initializers = new Map<string, ts.Expression>();
    const literals = new Map<string, ts.Expression>();
    for (const statement of file.statements) {
        if (!ts.isVariableStatement(statement)) {
            continue;
        }
        const { flags } = statement.declarationList;
        // `await using` sets the Const bit too.
        const isConst =
            (flags & ts.NodeFlags.Const) !== 0 &&
            (flags & ts.NodeFlags.Using) === 0;
        if (!isConst) {
            continue;
        }
        for (const decl of statement.declarationList.declarations) {
            if (!decl.initializer || !ts.isIdentifier(decl.name)) {
                continue;
            }
            initializers.set(decl.name.text, decl.initializer);
            if (isTrackableLiteral(decl.initializer)) {
                literals.set(decl.name.text, decl.initializer);
            }
        }
    }
    return { file, initializers, literals };
}

/** Length of the string a literal evaluates to, or null when the source
 *  alone cannot tell. */
function evaluatedLength(
    node: ts.Node,
    table: ConstTable,
    visited = new Set<string>(),
): number | null {
    if (ts.isStringLiteral(node) || ts.isNoSubstitutionTemplateLiteral(node)) {
        return node.text.length;
    }
    if (!ts.isTemplateExpression(node)) {
        return null;
    }
    let total = node.head.text.length;
    for (const span of node.templateSpans) {
        const length = interpolatedLength(span.expression, table, visited);
        if (length === null) {
            return null;
        }
        total += length + span.literal.text.length;
    }
    return total;
}

/** Length of what an interpolated expression evaluates to: a const literal
 *  (guarding circular references), or a string or number written in place. */
function interpolatedLength(
    expr: ts.Expression,
    table: ConstTable,
    visited: Set<string>,
): number | null {
    if (ts.isIdentifier(expr)) {
        const literal = table.literals.get(expr.text);
        if (visited.has(expr.text) || !literal) {
            return null;
        }
        visited.add(expr.text);
        const length = evaluatedLength(literal, table, visited);
        visited.delete(expr.text);
        return length;
    }
    if (ts.isStringLiteral(expr)) {
        return expr.text.length;
    }
    if (ts.isNumericLiteral(expr)) {
        return expr.getText(table.file).length;
    }
    return null;
}

/**
 * The const interpolations of a template expression, each with where its
 * value starts in the evaluated text. Resolution stops at the first
 * interpolation whose length is unknown, since every later offset would be
 * wrong; the ones before it are kept. Null when nothing resolves.
 */
export function resolveInterpolations(
    node: ts.Node,
    table: ConstTable,
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
            const literal = table.literals.get(expr.text);
            const length = literal ? evaluatedLength(literal, table) : null;
            if (!literal || length === null) {
                return settled();
            }
            resolutions.push({
                constLiteralSpan: spanOf(literal, table.file),
                evaluatedLength: length,
                evaluatedStart: offset,
                nestedResolutions:
                    resolveInterpolations(literal, table) ?? undefined,
            });
            offset += length;
        } else {
            const length = interpolatedLength(expr, table, new Set());
            if (length === null) {
                return settled();
            }
            offset += length;
        }
        offset += span.literal.text.length;
    }
    return settled();
}
