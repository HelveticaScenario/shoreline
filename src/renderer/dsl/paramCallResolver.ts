/**
 * Resolve a DSL call expression to the params its arguments set.
 *
 * Module calls — direct factories (`$saw(...)`, `$unstable.filter.lp(...)`)
 * and chain modules on audio signals (`.lpf(...)`) — resolve through the module
 * schemas, which carry each param's signal type, range, and default. Every
 * other API call — functions (`$setOutputGain`, `$phase.bend`, `$v.osc`),
 * methods (`.out`, `.send`, `.amp`, `.range`), and video chains — resolves
 * through the generated DSL lib's signatures (apiSignatures.ts), in the
 * family of its receiver (receiverFamily.ts), with ranges and defaults from
 * API_CONTROLS, then from the lib's JSDoc.
 */
import ts from 'typescript';

import { negatableNumber } from '../../shared/dsl/ast/literals';
import { dottedName, isPatternTag } from '../../shared/dsl/ast/names';
import type { Schemas } from '../../shared/dsl/schemaTypeResolver';
import type { ApiIndex, Family } from './apiSignatures';
import {
    API_CONTROLS,
    controlRef,
    hintedRef,
    schemaParamRef,
} from './apiParamControls';
import type { ParamRef, SchemaEntry } from './apiParamControls';
import { docHints } from './apiTypeReader';
import type {
    ApiParam,
    ApiSignature,
    DocHints,
    SlotStep,
} from './apiTypeReader';
import type { ParamLiteral } from './paramControls';
import { createFamilyReader, splitChain } from './receiverFamily';
import { schemaSlot } from './schemaSlots';

export interface ResolvedCall {
    /** The param the argument at `index` sets, or null when a control
     *  cannot drive it. */
    argParam(index: number): ParamRef | null;
    /** The param a value at `path` inside the argument at `index` sets — an
     *  element of a poly array, a tuple position, an option member — or
     *  null when a control cannot drive it. */
    slot(index: number, path: readonly SlotStep[]): ParamRef | null;
    /** The option object argument `index` accepts, by member key (null for
     *  a member a control cannot drive), or null when it takes none. */
    options(index: number): ReadonlyMap<string, ParamRef | null> | null;
    /** Names the first `argCount` arguments set, which an option object
     *  must not set again. */
    positionalNames(argCount: number): ReadonlySet<string>;
}

/** A resolver for calls in `file` against the module `schemas` and the DSL
 *  `api`. */
export function createCallResolver(
    file: ts.SourceFile,
    schemas: Schemas,
    api: ApiIndex,
): (call: ts.CallExpression) => ResolvedCall | null {
    const byName = new Map<string, SchemaEntry>();
    const byChainPath = new Map<string, SchemaEntry>();
    for (const s of schemas) {
        byName.set(s.name, s);
        byChainPath.set(s.name.startsWith('$') ? s.name.slice(1) : s.name, s);
    }
    const familyOf = createFamilyReader(api);
    let declared: Set<string> | undefined;
    /**
     * Whether a property chain starts at a DSL value: a call, a `$` pattern
     * template, a `$` global, or a name the patch itself declares. Rules out JS globals like `Math`,
     * whose methods (`Math.exp`) share names with DSL methods.
     */
    const hasDslRoot = (expr: ts.PropertyAccessExpression) => {
        let root: ts.Expression = expr;
        while (ts.isPropertyAccessExpression(root)) {
            root = root.expression;
        }
        if (ts.isCallExpression(root)) {
            return true;
        }
        if (ts.isTaggedTemplateExpression(root)) {
            return isPatternTag(root);
        }
        if (!ts.isIdentifier(root)) {
            return false;
        }
        if (root.text.startsWith('$')) {
            return true;
        }
        declared ??= declaredNames(file);
        return declared.has(root.text);
    };
    const schemaRef = (schema: SchemaEntry, name: string): ParamRef | null =>
        schemaParamRef(schema, name, name);
    const moduleCall = (
        schema: SchemaEntry,
        argToPositional: (i: number) => number | null,
        configSlot: number,
        chained: boolean,
    ): ResolvedCall => {
        const positionals = schema.positionalArgs ?? [];
        const props: Record<string, unknown> =
            schema.paramsSchema?.properties ?? {};
        const defs: Record<string, unknown> =
            (schema.paramsSchema as { $defs?: Record<string, unknown> })
                .$defs ?? {};
        const config = new Map<string, ParamRef | null>();
        for (const key of Object.keys(props)) {
            config.set(key, schemaRef(schema, key));
        }
        const isSignalParam = (name: string) =>
            schema.signalParams.some((p) => p.name === name);
        /** A slot inside param `name`. One reached through array elements
         *  alone takes the param's own control; one inside a nested member
         *  takes a control named after that member. */
        const paramSlot = (
            name: string,
            path: readonly SlotStep[],
        ): ParamRef | null => {
            if (path.length === 0 && isSignalParam(name)) {
                return schemaRef(schema, name);
            }
            const found = schemaSlot(props[name], path, defs);
            if (!found) {
                return null;
            }
            const members = path.flatMap((s) =>
                'member' in s ? [s.member] : [],
            );
            if (members.length === 0 && isSignalParam(name)) {
                return schemaRef(schema, name);
            }
            const prop = props[name] as { description?: unknown } | undefined;
            const description =
                members.length > 0
                    ? found.description
                    : typeof prop?.description === 'string'
                      ? prop.description
                      : null;
            return hintedRef(
                members.at(-1) ?? name,
                docHints(description ?? ''),
            );
        };
        return {
            argParam: (i) => {
                const pi = argToPositional(i);
                return pi === null
                    ? null
                    : schemaRef(schema, positionals[pi].name);
            },
            options: (i) => (i === configSlot ? config : null),
            slot: (i, path) => {
                const pi = argToPositional(i);
                if (pi !== null) {
                    return positionals[pi]
                        ? paramSlot(positionals[pi].name, path)
                        : null;
                }
                const [first, ...rest] = path;
                return i === configSlot && first && 'member' in first
                    ? paramSlot(first.member, rest)
                    : null;
            },
            positionalNames: (count) => {
                const names = new Set<string>();
                // A chain injects the first positional, so it is always set.
                if (chained && positionals[0]) {
                    names.add(positionals[0].name);
                }
                for (let i = 0; i < count; i++) {
                    const pi = argToPositional(i);
                    if (pi !== null && positionals[pi]) {
                        names.add(positionals[pi].name);
                    }
                }
                return names;
            },
        };
    };
    const apiCall = (
        family: Family,
        callee: string,
        sig: ApiSignature,
    ): ResolvedCall => {
        const controls = API_CONTROLS[family][callee] ?? {};
        const ref = (
            name: string,
            signal: boolean,
            hints: DocHints,
            required = false,
        ): ParamRef | null => {
            if (!signal) {
                return null;
            }
            const source = controls[name];
            return source
                ? controlRef(name, source, byName)
                : hintedRef(name, hints, required);
        };
        const paramAt = (i: number): ApiParam | undefined => {
            const last = sig[sig.length - 1];
            return i < sig.length ? sig[i] : last?.rest ? last : undefined;
        };
        return {
            argParam: (i) => {
                const p = paramAt(i);
                return p ? ref(p.name, p.signal, p.hints, !p.optional) : null;
            },
            slot: (i, path) => {
                const p = paramAt(i);
                const found = p?.slot(path);
                if (!p || !found) {
                    return null;
                }
                const members = path.flatMap((s) =>
                    'member' in s ? [s.member] : [],
                );
                const [first] = members;
                if (first === undefined) {
                    return ref(p.name, true, p.hints, !p.optional);
                }
                // A direct option member keeps the hints the option object
                // resolves for it, `@param` docs and forwarded ones included.
                const member =
                    members.length === 1 && 'member' in path[0]
                        ? p.options?.get(first)
                        : undefined;
                return ref(
                    members[members.length - 1],
                    true,
                    found.hints ?? member?.hints ?? {},
                );
            },
            options: (i) => {
                const p = paramAt(i);
                if (!p?.options) {
                    return null;
                }
                const members = new Map<string, ParamRef | null>();
                for (const [key, m] of p.options) {
                    members.set(key, ref(key, m.signal, m.hints));
                }
                return members;
            },
            positionalNames: () => new Set(),
        };
    };
    /** A chain module: the receiver is its first positional argument. */
    const moduleChain = (schema: SchemaEntry) => {
        const count = schema.positionalArgs?.length ?? 0;
        return moduleCall(
            schema,
            (i) => (i + 1 < count ? i + 1 : null),
            count - 1,
            true,
        );
    };

    return (call) => {
        const expr = call.expression;

        // A module factory or API function, named by its callee however it
        // is spread across lines.
        const calleeName = dottedName(expr);
        if (calleeName !== null) {
            const direct = byName.get(calleeName);
            if (direct) {
                const count = direct.positionalArgs?.length ?? 0;
                return moduleCall(
                    direct,
                    (i) => (i < count ? i : null),
                    count,
                    false,
                );
            }
            const fn = api.functions.get(calleeName);
            if (fn) {
                const family = api.videoFunctions.has(calleeName)
                    ? 'video'
                    : 'audio';
                return apiCall(family, calleeName, fn);
            }
        }

        if (!ts.isPropertyAccessExpression(expr) || !hasDslRoot(expr)) {
            return null;
        }
        // Audio and video members share names, so the receiver's family
        // picks which apply; an unknown receiver tries audio first.
        const { chain, receiver } = splitChain(expr, api);
        const known = familyOf(receiver);
        const schema =
            chain !== null && known !== 'video'
                ? byChainPath.get(chain)
                : undefined;
        if (schema) {
            return moduleChain(schema);
        }
        const name = expr.name.text;
        const families: Family[] = known ? [known] : ['audio', 'video'];
        for (const family of families) {
            const sig = api[family].methods.get(name);
            if (sig) {
                return apiCall(family, name, sig);
            }
        }
        return null;
    };
}

/** Every name a variable, parameter, or function declaration binds. */
function declaredNames(file: ts.SourceFile): Set<string> {
    const names = new Set<string>();
    const visit = (node: ts.Node) => {
        if (
            (ts.isVariableDeclaration(node) ||
                ts.isParameter(node) ||
                ts.isBindingElement(node) ||
                ts.isFunctionDeclaration(node)) &&
            node.name &&
            ts.isIdentifier(node.name)
        ) {
            names.add(node.name.text);
        }
        ts.forEachChild(node, visit);
    };
    visit(file);
    return names;
}

/** A numeric (optionally negated) or plain string literal, or null for any
 *  other expression. */
export function asLiteral(
    node: ts.Node,
    file: ts.SourceFile,
): ParamLiteral | null {
    // Only unary minus: control-literal validation rejects a unary plus.
    const value = negatableNumber(node);
    if (value !== null) {
        return { kind: 'number', text: node.getText(file), value };
    }
    if (ts.isStringLiteralLike(node)) {
        return { kind: 'string', text: node.getText(file), value: node.text };
    }
    return null;
}
