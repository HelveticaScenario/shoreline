/**
 * Recognising mini-notation pattern calls in the source: `$p(...)`,
 * `$p.s(...)`, `$p.arrange(...)`, and the chains built on them, followed
 * through `const` bindings.
 */
import ts from 'typescript';

import { constInitializerOf } from '../../shared/dsl/ast/consts';
import { ALIGNMENT_MODES } from './alignmentModes';

/** Pattern methods whose argument spans are recorded. */
export type PatternMethodName = 'fast' | 'slow' | 'struct' | 'beat' | 'glide';

/** The argument names each pattern method looks its spans up by, in
 *  positional order. */
export const PATTERN_METHOD_ARGS: Record<
    PatternMethodName,
    ReadonlyArray<string>
> = {
    beat: ['t', 'div'],
    fast: ['factor'],
    glide: ['g'],
    slow: ['factor'],
    struct: ['boolPattern'],
};

const ALIGNMENTS: ReadonlySet<string> = new Set(ALIGNMENT_MODES);

type PatternRoot = '$p' | '$p.s' | '$p.arrange';

/**
 * The pattern builder a method chain starts from, walking back through
 * chain links and `const` hops (a visited set guards circular consts), or
 * null when the chain does not start at one.
 */
function chainRoot(node: ts.Node): PatternRoot | null {
    const visited = new Set<string>();
    let n = node;
    for (;;) {
        if (ts.isCallExpression(n)) {
            const callee = n.expression;
            if (ts.isIdentifier(callee)) {
                return callee.text === '$p' ? '$p' : null;
            }
            if (!ts.isPropertyAccessExpression(callee)) {
                return null;
            }
            const name = callee.name.text;
            const receiver = callee.expression;
            if (
                (name === 's' || name === 'arrange') &&
                ts.isIdentifier(receiver) &&
                receiver.text === '$p'
            ) {
                return name === 's' ? '$p.s' : '$p.arrange';
            }
            n = receiver;
        } else if (ts.isPropertyAccessExpression(n)) {
            n = n.expression;
        } else if (ts.isIdentifier(n)) {
            const init = constInitializerOf(n);
            if (!init || visited.has(n.text)) {
                return null;
            }
            visited.add(n.text);
            n = init;
        } else {
            return null;
        }
    }
}

/** A direct `$p.s(source, scale)` call. */
export function isPsCall(call: ts.CallExpression): boolean {
    const callee = call.expression;
    return (
        ts.isPropertyAccessExpression(callee) &&
        callee.name.text === 's' &&
        ts.isIdentifier(callee.expression) &&
        callee.expression.text === '$p'
    );
}

/**
 * A `.trans(rhs)` link, or one with a mode (`.trans.in(rhs)`,
 * `.trans.squeeze(rhs)`, …), on a chain rooted at `$p.s(...)`.
 */
export function isTransCall(call: ts.CallExpression): boolean {
    const callee = call.expression;
    if (!ts.isPropertyAccessExpression(callee)) {
        return false;
    }
    let receiver = callee.expression;
    if (ALIGNMENTS.has(callee.name.text)) {
        if (
            !ts.isPropertyAccessExpression(receiver) ||
            receiver.name.text !== 'trans'
        ) {
            return false;
        }
        receiver = receiver.expression;
    } else if (callee.name.text !== 'trans') {
        return false;
    }
    return chainRoot(receiver) === '$p.s';
}

/** The pattern method a `.fast(…)`, `.slow(…)`, `.struct(…)`, `.beat(…)` or
 *  `.glide(…)` call on any pattern chain names, or null. */
export function patternMethodCallName(
    call: ts.CallExpression,
): PatternMethodName | null {
    const callee = call.expression;
    if (!ts.isPropertyAccessExpression(callee)) {
        return null;
    }
    const name = callee.name.text;
    if (!Object.hasOwn(PATTERN_METHOD_ARGS, name)) {
        return null;
    }
    return chainRoot(callee.expression) !== null
        ? (name as PatternMethodName)
        : null;
}
