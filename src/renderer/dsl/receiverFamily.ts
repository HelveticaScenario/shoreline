/**
 * Whether a patch expression evaluates to an audio or a video value, read
 * from the source alone: the generated DSL lib's return types, followed
 * along calls, chains, and `const` bindings. Picks between same-named audio
 * and video members, such as `.range` or `.$.fold`.
 */
import ts from 'typescript';

import { constBindings } from '../../shared/dsl/ast/consts';
import { dottedName } from '../../shared/dsl/ast/names';
import type { ApiIndex, Family } from './apiSignatures';

const MAX_DEPTH = 16;

/**
 * A reader of the family of expressions in `file`, or null for one whose
 * family the source alone cannot tell (a function parameter, a `let`).
 */
export function createFamilyReader(
    file: ts.SourceFile,
    api: ApiIndex,
): (expr: ts.Expression) => Family | null {
    const constInit = (name: string) => constBindings(file).get(name) ?? null;

    const familyOf = (expr: ts.Expression, depth: number): Family | null => {
        if (depth > MAX_DEPTH) {
            return null;
        }
        const next = (e: ts.Expression) => familyOf(e, depth + 1);
        if (
            ts.isParenthesizedExpression(expr) ||
            ts.isNonNullExpression(expr)
        ) {
            return next(expr.expression);
        }
        if (ts.isIdentifier(expr)) {
            const init = constInit(expr.text);
            if (init) {
                return next(init);
            }
            if (!expr.text.startsWith('$') || expr.text === '$v') {
                return null;
            }
            return api.videoValues.has(expr.text) ? 'video' : 'audio';
        }
        if (ts.isElementAccessExpression(expr)) {
            return next(expr.expression);
        }
        if (ts.isPropertyAccessExpression(expr)) {
            const name = dottedName(expr);
            if (name !== null && api.videoValues.has(name)) {
                return 'video';
            }
            return next(expr.expression);
        }
        if (!ts.isCallExpression(expr)) {
            return null;
        }
        const callee = expr.expression;
        const fn = dottedName(callee);
        const chain = ts.isPropertyAccessExpression(callee)
            ? splitChain(callee)
            : null;
        if (
            fn !== null &&
            !chain?.dollar &&
            (api.functions.has(fn) || api.videoFunctions.has(fn))
        ) {
            return api.videoFunctions.has(fn) ? 'video' : 'audio';
        }
        if (!chain) {
            // A bare `$` factory (`$saw(...)`) makes an audio module.
            return fn?.startsWith('$') && !constInit(fn) ? 'audio' : null;
        }
        const receiver = next(chain.receiver);
        if (!receiver) {
            return null;
        }
        const key = chain.dollar
            ? `${chain.dollar}.${chain.path.join('.')}`
            : chain.path[0];
        return api[receiver].returnsVideo.has(key) ? 'video' : 'audio';
    };
    return (expr) => familyOf(expr, 0);
}

/**
 * `recv.$.a.b` splits into receiver `recv`, dollar `$`, and path
 * `['a', 'b']`; a callee without a `$` / `$m` segment has receiver
 * `callee.expression` and no dollar.
 */
export function splitChain(callee: ts.PropertyAccessExpression): {
    receiver: ts.Expression;
    dollar: '$' | '$m' | null;
    path: string[];
} {
    const path: string[] = [];
    for (let e: ts.Expression = callee; ts.isPropertyAccessExpression(e); ) {
        const name = e.name.text;
        if (name === '$' || name === '$m') {
            return { dollar: name, path, receiver: e.expression };
        }
        path.unshift(name);
        e = e.expression;
    }
    return {
        dollar: null,
        path: [callee.name.text],
        receiver: callee.expression,
    };
}
