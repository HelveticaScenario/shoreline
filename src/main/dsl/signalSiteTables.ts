import type { ModuleSchema } from '@modular/core';
import type {
    ModuleSiteInfo,
    SignalSiteTables,
} from '../../shared/dsl/signalSites';
import { CHAIN_METHODS } from './chainMethods';
import { resolvableOutputs } from './outputResolution';
import { processModuleSchema, qualifiesForChain } from './paramsSchema';
import { signalParamsOf } from './signalParams';
import {
    PATTERN_TYPES,
    qualifiedMultiOutputInterfaceName,
} from './typescriptLibGen';

/** `$delay(input, time, { feedback })`: a helper called like a module. */
export const DELAY_SITE: ModuleSiteInfo = {
    buffers: [],
    chainable: true,
    monoOutputs: [],
    folds: [],
    inputs: { feedback: 'cv', input: 'cv', time: 'cv' },
    positional: ['input', 'time'],
    stereo: 'lift',
};

/**
 * The globals that are signals or make them without being a module call.
 * Patterns and panel controls are mono; `$input` pairs device channels.
 */
const GLOBAL_CHANNELS: SignalSiteTables['globals'] = {
    $: 'mono',
    $arrange: 'mono',
    $btn: 'mono',
    $c: 'collection',
    $clock: 'mono',
    $input: 'stereo',
    $inputMono: 'mono',
    $r: 'collection',
    $slider: 'mono',
    $toggleBtn: 'mono',
};

/** The params of `schema` that take a `$buffer`. */
function bufferParams(schema: ModuleSchema): string[] {
    const properties = (
        schema.paramsSchema as {
            properties?: Record<string, { $ref?: string }>;
        }
    ).properties;
    return Object.entries(properties ?? {})
        .filter(([, property]) => property.$ref === '#/$defs/Buffer')
        .map(([name]) => name);
}

/**
 * Describe the DSL to the editor's signal-site collector, in the names the
 * generated lib and the runtime use.
 */
export function buildSignalSiteTables(
    schemas: ModuleSchema[],
): SignalSiteTables {
    const tables: SignalSiteTables = {
        chainMethods: CHAIN_METHODS,
        globals: GLOBAL_CHANNELS,
        modules: { $delay: DELAY_SITE },
        // A pattern is the `$cycle` that plays it, which is mono.
        monoTypes: [...PATTERN_TYPES],
        roots: {},
    };

    for (const schema of schemas) {
        const processed = processModuleSchema(schema);
        const { flat, nested } = signalParamsOf(schema, processed);
        tables.modules[schema.name] = {
            buffers: bufferParams(schema),
            chainable: qualifiesForChain(processed),
            monoOutputs: (schema.outputs ?? [])
                .filter((output) => output.mono)
                .map((output) => output.name),
            // A mono module folds every signal it reads, nested ones too.
            folds: [
                ...(schema.signalParams ?? [])
                    .filter((param) => param.fold)
                    .map((param) => param.name),
                ...(schema.stereo === 'mono'
                    ? nested.map((param) => param.name)
                    : []),
            ],
            inputs: Object.fromEntries([
                ...flat.map((p) => [p.name, p.inputClass] as const),
                ...nested.map((p) => [p.name, 'cv'] as const),
            ]),
            positional: (schema.positionalArgs ?? []).map((p) => p.name),
            stereo: schema.stereo as ModuleSiteInfo['stereo'],
        };
        if ((schema.outputs ?? []).length > 1) {
            tables.roots[qualifiedMultiOutputInterfaceName(schema)] =
                resolvableOutputs(schema);
        }
        // A pattern is the root of its `$cycle`.
        if (schema.name === '$cycle') {
            for (const type of PATTERN_TYPES) {
                tables.roots[type] = resolvableOutputs(schema);
            }
        }
    }
    return tables;
}
