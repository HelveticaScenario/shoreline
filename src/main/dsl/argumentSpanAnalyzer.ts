/**
 * Argument Span Analyzer
 *
 * Finds the character span of each literal argument passed to a module
 * factory, a pattern builder (`$p`, `$p.s`), a `$p.s` chain link, or a
 * pattern method, keyed by the `line:column` V8 reports for the call so the
 * factory can look its spans up at runtime from `captureSourceLocation()`.
 * A `const` reference resolves to the const's literal, and a template that
 * interpolates const strings records where each lands (templateInterpolation.ts).
 *
 * Control calls are checked here too: their arguments must be literals
 * (controlCallValidation.ts).
 */
import ts from 'typescript';
import type { ModuleSchema } from '@modular/core';

import type {
    InterpolationResolutionMap,
    SourceSpan,
    SpanRegistry,
} from './sourceAnalysisTypes';
import { callSiteKey, v8CallAnchor } from '../../shared/dsl/ast/position';
import { forEachDescendant } from '../../shared/dsl/ast/walk';
import { findControlCalls } from '../../shared/dsl/controlCalls';
import { validateControlCall } from './controlCallValidation';
import {
    isPsCall,
    isSpChainCall,
    PATTERN_METHOD_ARGS,
    patternMethodCallName,
} from './patternChains';
import {
    buildConstTable,
    isTrackableLiteral,
    resolveInterpolations,
    spanOf,
} from './templateInterpolation';

/**
 * Schemas by every name a call can reach them through: the full name, its
 * last segment (`kick` of `$tr808.kick`, as a `.kick(...)` method call
 * names it), and the camel-cased form of the full name.
 */
function buildSchemaMap(schemas: ModuleSchema[]): Map<string, ModuleSchema> {
    const map = new Map<string, ModuleSchema>();
    for (const schema of schemas) {
        const parts = schema.name.split('.');
        map.set(parts[parts.length - 1], schema);
        const camel = schema.name.replace(
            /[^a-zA-Z0-9]+(.)?/g,
            (_match, chr: string | undefined) => (chr ? chr.toUpperCase() : ''),
        );
        map.set(camel, schema);
        map.set(schema.name, schema);
    }
    return map;
}

/** The name a call is matched against schemas by: the identifier of `f()`,
 *  or the method name of `a.f()`. */
function calledName(call: ts.CallExpression): string | null {
    const callee = call.expression;
    if (ts.isIdentifier(callee)) {
        return callee.text;
    }
    if (ts.isPropertyAccessExpression(callee)) {
        return callee.name.text;
    }
    return null;
}

/**
 * Walk every call in `file` and record the spans of its trackable
 * arguments, with the interpolations each template argument resolves.
 */
export function analyzeArgumentSpans(
    file: ts.SourceFile,
    schemas: ModuleSchema[],
): {
    registry: SpanRegistry;
    interpolationResolutions: InterpolationResolutionMap;
} {
    const registry: SpanRegistry = new Map();
    const interpolationResolutions: InterpolationResolutionMap = new Map();
    const schemaMap = buildSchemaMap(schemas);
    const consts = buildConstTable(file);
    const controlKinds = new Map(
        findControlCalls(file).map((c) => [c.call, c.kind]),
    );

    /**
     * Record `node`'s span in `args` under `name`: a literal's own span, or
     * the literal a const reference holds. Returns whether it was trackable.
     */
    const record = (
        args: Map<string, SourceSpan>,
        name: string,
        node: ts.Node,
    ): boolean => {
        const literal = isTrackableLiteral(node)
            ? node
            : ts.isIdentifier(node)
              ? consts.literals.get(node.text)
              : undefined;
        if (!literal) {
            return false;
        }
        const span = spanOf(literal, file);
        args.set(name, span);
        const resolutions = resolveInterpolations(literal, consts);
        if (resolutions) {
            interpolationResolutions.set(
                `${span.start}:${span.end}`,
                resolutions,
            );
        }
        return true;
    };

    forEachDescendant(file, (node) => {
        if (!ts.isCallExpression(node)) {
            return;
        }
        const call = node;
        const args = call.arguments;

        const controlKind = controlKinds.get(call);
        if (controlKind) {
            validateControlCall(call, controlKind, file);
            return;
        }

        // `$p(source)` and `$p.s(source, scale)`: the source literal's span
        // rides on the returned pattern to whichever factory receives it.
        // The scale is configuration, not highlighted. V8 places `$p.s` at
        // its `s`, and a bare `$p` call at its start.
        const funcName = calledName(call);
        const isPs = isPsCall(call);
        if (funcName === '$p' || isPs) {
            const argsMap = new Map<string, SourceSpan>();
            if (args.length < 1 || !record(argsMap, 'source', args[0])) {
                return;
            }
            const anchor = isPs
                ? v8CallAnchor(call, file)
                : call.getStart(file);
            registry.set(callSiteKey(file, anchor), {
                args: argsMap,
                moduleType: isPs ? '$p.s' : '$p',
            });
            return;
        }

        // `.add(rhs)` / `.sub(rhs)` links of a `$p.s` chain, read back by the
        // chain method under `rhs`.
        if (isSpChainCall(call, consts.initializers)) {
            const argsMap = new Map<string, SourceSpan>();
            if (args.length < 1 || !record(argsMap, 'rhs', args[0])) {
                return;
            }
            registry.set(callSiteKey(file, v8CallAnchor(call, file)), {
                args: argsMap,
                moduleType: '$p.s.chain',
            });
            return;
        }

        // Pattern methods, each argument under the name the method reads it
        // back by.
        const patternMethod = patternMethodCallName(call, consts.initializers);
        if (patternMethod) {
            const argNames = PATTERN_METHOD_ARGS[patternMethod];
            const argsMap = new Map<string, SourceSpan>();
            for (let i = 0; i < argNames.length && i < args.length; i++) {
                record(argsMap, argNames[i], args[i]);
            }
            if (argsMap.size > 0) {
                registry.set(callSiteKey(file, v8CallAnchor(call, file)), {
                    args: argsMap,
                    moduleType: `.${patternMethod}`,
                });
            }
            return;
        }

        const schema = funcName ? schemaMap.get(funcName) : undefined;
        if (!schema) {
            return;
        }
        const argsMap = new Map<string, SourceSpan>();

        // Positional arguments by their schema names; an array literal
        // records each element as `name.0`, `name.1`, ….
        const positionalArgs = schema.positionalArgs || [];
        for (let i = 0; i < positionalArgs.length && i < args.length; i++) {
            const { name } = positionalArgs[i];
            const arg = args[i];
            if (ts.isArrayLiteralExpression(arg)) {
                arg.elements.forEach((element, j) =>
                    record(argsMap, `${name}.${j}`, element),
                );
            } else {
                record(argsMap, name, arg);
            }
        }

        // The config object after the positional arguments, by key.
        const config = args[positionalArgs.length];
        if (config && ts.isObjectLiteralExpression(config)) {
            for (const prop of config.properties) {
                if (ts.isPropertyAssignment(prop)) {
                    const key = prop.name.getText(file);
                    if (key !== 'id') {
                        record(argsMap, key, prop.initializer);
                    }
                } else if (ts.isShorthandPropertyAssignment(prop)) {
                    if (prop.name.text !== 'id') {
                        record(argsMap, prop.name.text, prop.name);
                    }
                }
            }
        }

        if (argsMap.size > 0) {
            registry.set(callSiteKey(file, v8CallAnchor(call, file)), {
                args: argsMap,
                moduleType: schema.name,
            });
        }
    });

    return { interpolationResolutions, registry };
}
