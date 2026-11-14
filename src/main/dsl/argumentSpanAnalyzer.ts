/**
 * Argument Span Analyzer
 *
 * Finds the character span of each literal argument passed to a module
 * factory, the pattern builder `$`, a `.trans` chain link, or a
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
import {
    dottedName,
    isPatternTag,
    propertyName,
} from '../../shared/dsl/ast/names';
import { callSiteKey, v8CallAnchor } from '../../shared/dsl/ast/position';
import { forEachDescendant } from '../../shared/dsl/ast/walk';
import { findControlCalls } from '../../shared/dsl/controlCalls';
import { validateControlCall } from './controlCallValidation';
import {
    isTransCall,
    PATTERN_METHOD_ARGS,
    patternMethodCallName,
} from './patternChains';
import {
    constLiteralOf,
    isTrackableLiteral,
    resolveInterpolations,
    spanOf,
} from './templateInterpolation';

/**
 * Schemas by the names a direct call reaches them through: the full name and
 * its camel-cased form.
 */
function buildSchemaMap(schemas: ModuleSchema[]): Map<string, ModuleSchema> {
    const map = new Map<string, ModuleSchema>();
    for (const schema of schemas) {
        const camel = schema.name.replace(
            /[^a-zA-Z0-9]+(.)?/g,
            (_match, chr: string | undefined) => (chr ? chr.toUpperCase() : ''),
        );
        map.set(camel, schema);
        map.set(schema.name, schema);
    }
    return map;
}

/**
 * The module a chain call names: `sig.lpf(...)` is `$lpf` and
 * `sig.tr808.kick(...)` is `$tr808.kick`, the longest trailing path of the
 * callee that names a module taking the receiver as its first argument.
 */
function chainedSchema(
    callee: ts.Expression,
    schemaMap: Map<string, ModuleSchema>,
): ModuleSchema | undefined {
    const names: string[] = [];
    let found: ModuleSchema | undefined;
    let node = callee;
    while (ts.isPropertyAccessExpression(node)) {
        names.unshift(node.name.text);
        node = node.expression;
        const schema = schemaMap.get('$' + names.join('.'));
        if (schema?.positionalArgs?.length) {
            found = schema;
        }
    }
    return found;
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
              ? constLiteralOf(node)
              : undefined;
        if (!literal) {
            return false;
        }
        const span = spanOf(literal, file);
        args.set(name, span);
        const resolutions = resolveInterpolations(literal, file);
        if (resolutions) {
            interpolationResolutions.set(
                `${span.start}:${span.end}`,
                resolutions,
            );
        }
        return true;
    };

    forEachDescendant(file, (node) => {
        // `` $`source` ``: V8 places a tagged template at its template, not
        // its tag.
        if (ts.isTaggedTemplateExpression(node)) {
            if (isPatternTag(node)) {
                const argsMap = new Map<string, SourceSpan>();
                record(argsMap, 'source', node.template);
                registry.set(callSiteKey(file, node.template.getStart(file)), {
                    args: argsMap,
                    kind: 'pattern',
                });
            }
            return;
        }
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

        // `$(source)`: the source literal's span rides on the returned
        // pattern to whichever factory receives it.
        const funcName = calledName(call);
        if (ts.isIdentifier(call.expression) && funcName === '$') {
            const argsMap = new Map<string, SourceSpan>();
            if (args.length < 1 || !record(argsMap, 'source', args[0])) {
                return;
            }
            registry.set(callSiteKey(file, v8CallAnchor(call, file)), {
                args: argsMap,
                kind: 'pattern',
            });
            return;
        }

        // `.trans(rhs)` links of a pattern chain, read back by the
        // chain method under `rhs`.
        if (isTransCall(call)) {
            const argsMap = new Map<string, SourceSpan>();
            if (args.length < 1 || !record(argsMap, 'rhs', args[0])) {
                return;
            }
            registry.set(callSiteKey(file, v8CallAnchor(call, file)), {
                args: argsMap,
                kind: 'pattern',
            });
            return;
        }

        // Pattern methods, each argument under the name the method reads it
        // back by.
        const patternMethod = patternMethodCallName(call);
        if (patternMethod) {
            const argNames = PATTERN_METHOD_ARGS[patternMethod];
            const argsMap = new Map<string, SourceSpan>();
            for (let i = 0; i < argNames.length && i < args.length; i++) {
                record(argsMap, argNames[i], args[i]);
            }
            if (argsMap.size > 0) {
                registry.set(callSiteKey(file, v8CallAnchor(call, file)), {
                    args: argsMap,
                    kind: 'pattern',
                });
            }
            return;
        }

        // A dotted callee names its module in full (`$tr808.kick`), a bare
        // one by its identifier. Any other callee is a chain call, whose
        // receiver is the module's first positional argument.
        const fullName = dottedName(call.expression);
        const direct =
            (fullName !== null ? schemaMap.get(fullName) : undefined) ??
            (ts.isIdentifier(call.expression) && funcName
                ? schemaMap.get(funcName)
                : undefined);
        const schema = direct ?? chainedSchema(call.expression, schemaMap);
        if (!schema) {
            return;
        }
        const argsMap = new Map<string, SourceSpan>();

        // Positional arguments by their schema names; an array literal
        // records each element as `name.0`, `name.1`, ….
        const positionalArgs = (schema.positionalArgs || []).slice(
            direct ? 0 : 1,
        );
        for (let i = 0; i < positionalArgs.length && i < args.length; i++) {
            const { name } = positionalArgs[i];
            const arg = args[i];
            if (ts.isArrayLiteralExpression(arg)) {
                // A spread puts every later element at an index the source
                // cannot tell.
                for (const [j, element] of arg.elements.entries()) {
                    if (ts.isSpreadElement(element)) {
                        break;
                    }
                    record(argsMap, `${name}.${j}`, element);
                }
            } else {
                record(argsMap, name, arg);
            }
        }

        // The config object after the positional arguments, by key.
        const config = args[positionalArgs.length];
        if (config && ts.isObjectLiteralExpression(config)) {
            for (const prop of config.properties) {
                if (ts.isPropertyAssignment(prop)) {
                    const key = propertyName(prop);
                    if (key !== null && key !== 'id') {
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
                kind: 'module',
            });
        }
    });

    return { interpolationResolutions, registry };
}
