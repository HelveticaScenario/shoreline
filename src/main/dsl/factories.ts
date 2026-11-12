import type { ModuleSchema } from '@modular/core';
import { deriveChannelCount } from '@modular/core';
import type { AnyCollection, ScopeConfig } from './GraphBuilder';
import {
    BaseCollection,
    GraphBuilder,
    Collection,
    CollectionWithRange,
} from './GraphBuilder';
import type { SourceSpan } from '../../shared/dsl/spanTypes';
import type { CallSiteKey, SpanRegistry } from './analyzeSource';
import type { CallSiteSpans } from './sourceAnalysisTypes';
import {
    chooseOutput,
    elementSlot,
    type OutputClass,
} from '../../shared/dsl/outputResolutionRule';
import './chainMethods';
import { outputName } from './outputNames';
import { markRoot, resolvableOutputs, type CallSite } from './outputResolution';
import { processModuleSchema } from './paramsSchema';
import { isPattern, patternPayload } from './patterns';
import { isSignalGroupLike } from './signalGroups';
import { signalParamsOf } from './signalParams';
import {
    captureSourceLocation,
    setDSLWrapperLineOffset,
} from './captureSourceLocation';

// Re-export so existing call sites (e.g. executor.ts) keep working.
export { captureSourceLocation, setDSLWrapperLineOffset };

/**
 * Key used for internal metadata field storing argument source spans.
 * Must match modular_core::types::ARGUMENT_SPANS_KEY in Rust.
 */
const ARGUMENT_SPANS_KEY = '__argument_spans';

/**
 * Active span registry for the current DSL execution.
 * Set by executor.ts before running user code, cleared after.
 */
let activeSpanRegistry: SpanRegistry | null = null;

/**
 * Set the active span registry for argument span capture.
 * Called by executor.ts before and after DSL execution.
 */
export function setActiveSpanRegistry(registry: SpanRegistry | null): void {
    activeSpanRegistry = registry;
}

/**
 * Type for argument spans attached to module params
 */
export type ArgumentSpans = Record<string, SourceSpan>;

/** A `wav` param value already resolved to the engine's reference. */
function isWavRef(value: unknown): boolean {
    return (
        typeof value === 'object' &&
        value !== null &&
        'type' in value &&
        value.type === 'wav_ref'
    );
}

/**
 * The editor spans of the sources of a pattern in param `paramName`, under
 * the keys the engine reports its highlights by: the param's name for a
 * `$p(...)`, and `<param>.<i>` for source `i` of any other pattern, whose
 * flat `argument_spans` follow the engine's per-source order.
 */
function patternArgumentSpans(
    paramName: string,
    value: unknown,
): ArgumentSpans {
    if (!isPattern(value)) {
        return {};
    }
    const { argument_span: single, argument_spans: perSource } = value as {
        argument_span?: SourceSpan;
        argument_spans?: readonly SourceSpan[];
    };
    if (!perSource) {
        return single ? { [paramName]: single } : {};
    }
    return Object.fromEntries(
        perSource.flatMap((span, i) =>
            span.start !== 0 || span.end !== 0
                ? [[`${paramName}.${i}`, span]]
                : [],
        ),
    );
}

/** The spans the analyzer recorded for the call at `sourceLocation`. */
function callSiteSpans(
    sourceLocation: { line: number; column: number } | undefined,
): CallSiteSpans | undefined {
    if (!activeSpanRegistry || !sourceLocation) {
        return undefined;
    }
    const key: CallSiteKey = `${sourceLocation.line}:${sourceLocation.column}`;
    return activeSpanRegistry.get(key);
}

/**
 * The argument spans of the `moduleType` call at `sourceLocation`, as a plain
 * object for the engine. A pattern's `$cycle` runs at the call that built the
 * pattern, whose spans are that call's own and not the module's.
 */
function captureArgumentSpans(
    sourceLocation: { line: number; column: number } | undefined,
    moduleType: string,
): ArgumentSpans | undefined {
    const entry = callSiteSpans(sourceLocation);
    return entry?.moduleType === moduleType
        ? Object.fromEntries(entry.args)
        : undefined;
}

/**
 * Look up a single argument span by source location and arg name.
 *
 * Exported for `$p()` so it can embed its own source literal span into the
 * returned `ParsedPattern` — highlighting is then the $p function's own
 * responsibility rather than something each module factory must unwrap.
 */
export function lookupArgumentSpan(
    sourceLocation: { line: number; column: number } | undefined,
    argName: string,
): SourceSpan | undefined {
    return callSiteSpans(sourceLocation)?.args.get(argName);
}

/**
 * Throw a module-and-line formatted error if any number nested anywhere in
 * `params` is non-finite. The N-API boundary rejects NaN/Infinity with a
 * context-free conversion error, so this must run before any native call
 * (deriveChannelCount) that receives the params.
 */
function assertFiniteNumericParams(
    moduleName: string,
    params: unknown,
    sourceLocation: { line: number; column: number } | undefined,
): void {
    const walk = (value: unknown, path: string): void => {
        if (typeof value === 'number') {
            if (!Number.isFinite(value)) {
                const loc = sourceLocation
                    ? ` at line ${sourceLocation.line}`
                    : '';
                throw new Error(
                    `${moduleName}${loc}: parameter \`${path}\` must be a finite number, got ${value}`,
                );
            }
            return;
        }
        if (Array.isArray(value)) {
            value.forEach((v, i) => walk(v, `${path}[${i}]`));
            return;
        }
        if (typeof value === 'object' && value !== null) {
            for (const [k, v] of Object.entries(value)) {
                walk(v, path === '' ? k : `${path}.${k}`);
            }
        }
    };
    walk(params, '');
}

/**
 * Derive a module's channel count with the guards every caller needs:
 * `assertFiniteNumericParams` first (the N-API boundary rejects NaN/Infinity
 * with a context-free conversion error, so the finite check must precede the
 * native call), then module-and-line formatting for any deserialization
 * errors. Returns the derived channel count, or undefined when the module
 * has no derivation.
 */
export function deriveChannelCountChecked(
    moduleName: string,
    params: unknown,
    sourceLocation: { line: number; column: number } | undefined,
): number | undefined {
    assertFiniteNumericParams(moduleName, params, sourceLocation);
    const deriveResult = deriveChannelCount(moduleName, params);
    if (deriveResult.errors && deriveResult.errors.length > 0) {
        const messages = deriveResult.errors
            .map((e: { message: string }) => e.message)
            .join('; ');
        const loc = sourceLocation ? ` at line ${sourceLocation.line}` : '';
        throw new Error(`${moduleName}${loc}: ${messages}`);
    }
    return deriveResult.channelCount;
}

// Return type for module factories: one output is a collection; several are
// the root collection with every output by name as `_<name>`.
type PolyOutput = Collection | CollectionWithRange;
type MultiOutput = PolyOutput & {
    readonly [output: `_${string}`]: PolyOutput;
};
type ModuleReturn = PolyOutput | MultiOutput;

type FactoryFunction = (...args: any[]) => ModuleReturn;

interface NamespaceTree {
    [key: string]: NamespaceTree | FactoryFunction;
}

function sanitizeIdentifier(name: string): string {
    let id = name.replace(
        /[^a-zA-Z0-9_$]+(.)?/g,
        (_match, chr: string | undefined) => (chr ? chr.toUpperCase() : ''),
    );
    if (!/^[A-Za-z_$]/.test(id)) {
        id = `_${id}`;
    }
    return id || '_';
}

/**
 * Build a nested namespace tree from module schemas
 * Mirrors the logic in typescriptLibGen.ts buildTreeFromSchemas()
 */
function buildNamespaceTree(
    schemas: ModuleSchema[],
    factoryMap: Record<string, FactoryFunction>,
): NamespaceTree {
    const tree: NamespaceTree = {};

    for (const schema of schemas) {
        const fullName = schema.name.trim();
        const parts = fullName.split('.').filter((p) => p.length > 0);

        const factoryName = sanitizeIdentifier(fullName);
        const factory = factoryMap[factoryName];

        if (parts.length === 1) {
            // No namespace, add to root
            tree[parts[0]] = factory;
        } else {
            // Navigate/create namespace hierarchy
            const className = parts[parts.length - 1];
            const namespacePath = parts.slice(0, -1);

            let current: NamespaceTree = tree;
            for (const ns of namespacePath) {
                if (!current[ns]) {
                    current[ns] = {};
                } else if (typeof current[ns] === 'function') {
                    throw new Error(
                        `Namespace collision: ${ns} is both a module and a namespace`,
                    );
                }
                current = current[ns];
            }

            if (
                current[className] &&
                typeof current[className] !== 'function'
            ) {
                throw new Error(
                    `Module name collision: ${className} already exists as a namespace`,
                );
            }
            current[className] = factory;
        }
    }

    return tree;
}

/**
 * DSL Context holds the builder and provides factory functions
 */
export class DSLContext {
    factories: Record<string, FactoryFunction> = {};
    namespaceTree: NamespaceTree = {};
    private builder: GraphBuilder;
    private resolveWav: (module: string, path: unknown) => unknown = (
        module,
    ) => {
        throw new Error(`${module}: WAV loading is not available`);
    };

    constructor(schemas: ModuleSchema[]) {
        this.builder = new GraphBuilder(schemas);

        // Build flat factory map (internal use for tree building)
        for (const schema of schemas) {
            const factoryName = sanitizeIdentifier(schema.name);
            this.factories[factoryName] = this.createFactory(schema);
        }

        // Register factories with the builder for internal use (late binding)
        // This allows GraphBuilder methods like .amplitude(), .shift(), .range() to use factories
        const factoryMap = new Map<string, FactoryFunction>();
        for (const schema of schemas) {
            factoryMap.set(
                schema.name,
                this.factories[sanitizeIdentifier(schema.name)],
            );
        }
        this.builder.setFactoryRegistry(factoryMap);

        // Build namespace tree (only way to access modules)
        this.namespaceTree = buildNamespaceTree(schemas, this.factories);
    }

    /**
     * Sets how a `wav` param's path becomes the engine's reference to the
     * decoded file; `module` names the module in errors.
     */
    setWavResolver(resolve: (module: string, path: unknown) => unknown): void {
        this.resolveWav = resolve;
    }

    /**
     * Prepare a signal param's value for an input of class `inputClass`:
     * resolve every module root in the value itself, array elements, and the
     * contents of a `$gN` group, and spread the collections inside an array.
     * `slot` names the value in resolution records.
     */
    private signalParam(
        value: unknown,
        inputClass: OutputClass,
        slot: string,
        sourceLocation: CallSite | undefined,
    ): unknown {
        if (isSignalGroupLike(value)) {
            return {
                ...value,
                signals: this.signalParam(
                    value.signals,
                    inputClass,
                    slot,
                    sourceLocation,
                ),
            };
        }
        const resolve = (v: unknown, s: string): unknown => {
            return this.builder.outputResolver.resolve(
                v,
                inputClass,
                sourceLocation,
                s,
            );
        };
        if (Array.isArray(value)) {
            return value.flatMap((v, i) => {
                const resolved = resolve(v, elementSlot(slot, i));
                return resolved instanceof BaseCollection
                    ? resolved.items
                    : [resolved];
            });
        }
        return resolve(value, slot);
    }

    /**
     * Create a module factory function that returns outputs directly
     */
    private createFactory(schema: ModuleSchema): FactoryFunction {
        const outputs = schema.outputs || [];
        const rootOutputs = resolvableOutputs(schema);
        const processed = processModuleSchema(schema);
        const scaleParamNames = processed.params
            .filter((p) => p.isScaleSignalInput)
            .map((p) => p.name);
        const wavParamNames = processed.params
            .filter((p) => p.isWavInput)
            .map((p) => p.name);
        const signalParams = signalParamsOf(schema, processed);
        const signalParamNames = new Set(
            [...signalParams.flat, ...signalParams.nested].map((p) => p.name),
        );

        return (...args: any[]): ModuleReturn => {
            const sourceLocation = captureSourceLocation();
            const argumentSpans = captureArgumentSpans(
                sourceLocation,
                schema.name,
            );
            const positionalArgs = schema.positionalArgs || [];
            const params: Record<string, any> = {};
            let config: any = {};
            let id: string | undefined;

            // Extract positional args
            for (let i = 0; i < positionalArgs.length; i++) {
                if (i < args.length) {
                    params[positionalArgs[i].name] = args[i];
                }
            }

            // The remaining arg (if any) is config.
            if (args.length > positionalArgs.length) {
                config = args[positionalArgs.length];
            }

            if (config) {
                const { id: configId, ...restConfig } = config;
                id = configId;
                // Merge other config params
                for (const key of Object.keys(restConfig)) {
                    params[key] = restConfig[key];
                }
            }

            for (const { inputClass, name } of signalParams.flat) {
                params[name] = this.signalParam(
                    params[name],
                    inputClass,
                    name,
                    sourceLocation,
                );
            }
            for (const { mapper, name } of signalParams.nested) {
                if (params[name] !== undefined) {
                    params[name] = mapper(params[name], name, (value, path) =>
                        this.signalParam(value, 'cv', path, sourceLocation),
                    );
                }
            }

            // In a param that takes no signal a pattern is the pattern
            // itself, where anywhere else it reads as its `$cycle`.
            for (const [name, value] of Object.entries(params)) {
                if (!signalParamNames.has(name)) {
                    params[name] = patternPayload(value);
                }
            }

            // The engine only accepts a signal scale; a spec string is
            // shorthand for the notes of `$chord(spec)`.
            for (const name of scaleParamNames) {
                if (typeof params[name] === 'string') {
                    params[name] = this.builder.getFactory('$chord')(
                        params[name],
                    );
                }
            }

            // A `wav` param is a path; the reference the engine reads also
            // carries the file's channel count, so the file loads here.
            for (const name of wavParamNames) {
                if (!isWavRef(params[name])) {
                    params[name] = this.resolveWav(schema.name, params[name]);
                }
            }

            // A pattern carries the spans of its own source literals, so
            // highlighting follows it through a `const`.
            const mergedArgumentSpans: ArgumentSpans = {
                ...(argumentSpans ?? {}),
            };
            for (const [paramName, value] of Object.entries(params)) {
                Object.assign(
                    mergedArgumentSpans,
                    patternArgumentSpans(paramName, value),
                );
            }

            // Attach argument spans to params if available
            // This allows Rust-side modules to access source locations for highlighting
            if (Object.keys(mergedArgumentSpans).length > 0) {
                params[ARGUMENT_SPANS_KEY] = mergedArgumentSpans;
            }

            // Create the module node internally, passing source location
            const node = this.builder.addModule(
                schema.name,
                id,
                sourceLocation,
            );

            // Set all params
            for (const [key, value] of Object.entries(params)) {
                if (value !== undefined) {
                    node._setParam(key, value);
                }
            }

            // Derive channel count from params using Rust-side derivation (backed by LRU cache)
            // This handles modules with custom derivation logic (like mix, seq)
            // As well as standard inference from PolySignal inputs
            const channelCount = deriveChannelCountChecked(
                schema.name,
                node.getParamsSnapshot(),
                sourceLocation,
            );
            if (channelCount !== undefined) {
                node._setDerivedChannelCount(channelCount);
            }

            // Return based on output configuration
            if (outputs.length === 0) {
                // No outputs - return empty object (shouldn't happen in practice)
                return {} as MultiOutput;
            } else if (outputs.length === 1) {
                return node._output(outputs[0].name);
            }
            // Multiple outputs: the root holds every output by name as
            // `_<name>`, each a separate untagged object. Read as a signal
            // itself, the root is the output a `cv` input takes.
            const named: Record<string, Collection | CollectionWithRange> = {};
            let root: Collection | CollectionWithRange;
            const cvOutput = chooseOutput(rootOutputs, 'cv');
            for (const output of outputs) {
                const name = outputName(output.name);
                named[name] = node._output(output.name);
                if (name === cvOutput) {
                    root = node._output(output.name);
                }
            }
            markRoot(root!, { outputs: rootOutputs, values: named });
            return root! as MultiOutput;
        };
    }

    /**
     * Get the builder instance
     */
    getBuilder(): GraphBuilder {
        return this.builder;
    }

    scope<T extends AnyCollection>(target: T, config?: ScopeConfig): T {
        this.builder.addScope(target.items, config, captureSourceLocation());
        return target;
    }
}

/**
 * Helper function to convert Hz to V/oct
 * V/oct = log2(Hz / C4) where C4 = 261.6255653005986 Hz
 * Convention: 0V = C4 = MIDI 60 = ~261.626 Hz
 */
const C4_HZ = 261.6255653005986; // 440 / 2^(9/12)

export function hz(frequency: number): number {
    if (frequency <= 0) {
        throw new Error('Frequency must be positive');
    }
    return Math.log2(frequency / C4_HZ);
}

/**
 * Note name to V/oct conversion
 * Supports notes like "c4", "c#4", "db4", etc.
 */
export function note(noteName: string): number {
    const noteRegex = /^([a-g])([#b]?)(-?\d+)?$/i;
    const match = noteName.toLowerCase().match(noteRegex);

    if (!match) {
        throw new Error(`Invalid note name: ${noteName}`);
    }

    const [, noteLetter, accidental, octaveStr] = match;
    const octave = octaveStr ? parseInt(octaveStr, 10) : 3;

    // Map note letters to semitones (C = 0)
    const noteMap: Record<string, number> = {
        a: 9,
        b: 11,
        c: 0,
        d: 2,
        e: 4,
        f: 5,
        g: 7,
    };

    let semitone = noteMap[noteLetter];

    // Apply accidentals
    if (accidental === '#') {
        semitone += 1;
    } else if (accidental === 'b') {
        semitone -= 1;
    }

    // Calculate frequency: C4 = 261.6255653005986 Hz (middle C)
    const semitonesFromC4 = (octave - 4) * 12 + semitone;
    const frequency = C4_HZ * 2 ** (semitonesFromC4 / 12);

    return hz(frequency);
}

/**
 * Convert BPM (beats per minute) to V/oct frequency
 * BPM is tempo, where 1 beat = 1 quarter note
 * At 120 BPM, that's 2 beats per second = 2 Hz
 */
export function bpm(beatsPerMinute: number): number {
    if (beatsPerMinute <= 0) {
        throw new Error('BPM must be positive');
    }
    // Convert BPM to Hz: Hz = BPM / 60
    const frequency = beatsPerMinute / 60;
    return hz(frequency);
}
