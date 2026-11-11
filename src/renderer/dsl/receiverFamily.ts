/**
 * Whether a patch expression evaluates to an audio or a video value, read
 * from the source alone: the generated DSL lib's return types, followed
 * along calls, chains, and `const` bindings. Picks between same-named audio
 * and video members, such as `.range` or `.fold`.
 */
import ts from 'typescript';

import { constInitializerOf } from '../../shared/dsl/ast/consts';
import { dottedName } from '../../shared/dsl/ast/names';
import type { ApiIndex, Family } from './apiSignatures';

const MAX_DEPTH = 16;

/**
 * A reader of the family of an expression, or null for one whose family the
 * source alone cannot tell (a function parameter, a `let`).
 */
export function createFamilyReader(
    api: ApiIndex,
): (expr: ts.Expression) => Family | null {
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
            const init = constInitializerOf(expr);
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
        if (
            fn !== null &&
            (api.functions.has(fn) || api.videoFunctions.has(fn))
        ) {
            return api.videoFunctions.has(fn) ? 'video' : 'audio';
        }
        if (!ts.isPropertyAccessExpression(callee)) {
            // A bare `$` factory (`$saw(...)`) makes an audio module.
            const shadowed =
                ts.isIdentifier(callee) && constInitializerOf(callee) !== null;
            return fn?.startsWith('$') && !shadowed ? 'audio' : null;
        }
        const chain = splitChain(callee, api);
        const receiver = next(chain.receiver);
        if (!receiver) {
            return null;
        }
        if (receiver === 'audio' && chain.chain !== null) {
            return 'audio';
        }
        return api[receiver].returnsVideo.has(callee.name.text)
            ? 'video'
            : 'audio';
    };
    return (expr) => familyOf(expr, 0);
}

/**
 * The receiver of method call `callee`, and the chain module it calls if
 * read as an audio chain: `recv.mi.chorus` splits into receiver `recv` and
 * path `mi.chorus`. The longest trailing path that names a chain module
 * wins; without one the receiver is `callee.expression` and `chain` null.
 */
export function splitChain(
    callee: ts.PropertyAccessExpression,
    api: ApiIndex,
): { receiver: ts.Expression; chain: string | null } {
    let found: { receiver: ts.Expression; chain: string } | undefined;
    const names: string[] = [];
    for (let e: ts.Expression = callee; ts.isPropertyAccessExpression(e); ) {
        names.unshift(e.name.text);
        e = e.expression;
        const path = names.join('.');
        if (api.audio.chains.has(path)) {
            found = { chain: path, receiver: e };
        }
    }
    return found ?? { chain: null, receiver: callee.expression };
}
