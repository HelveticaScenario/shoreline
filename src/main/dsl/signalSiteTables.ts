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
    chainable: true,
    inputs: { feedback: 'cv', input: 'cv', time: 'cv' },
    positional: ['input', 'time'],
};

/**
 * Describe the DSL to the editor's signal-site collector, in the names the
 * generated lib and the runtime use.
 */
export function buildSignalSiteTables(
    schemas: ModuleSchema[],
): SignalSiteTables {
    const tables: SignalSiteTables = {
        chainMethods: CHAIN_METHODS,
        modules: { $delay: DELAY_SITE },
        roots: {},
    };

    for (const schema of schemas) {
        const processed = processModuleSchema(schema);
        const { flat, nested } = signalParamsOf(schema, processed);
        tables.modules[schema.name] = {
            chainable: qualifiesForChain(processed),
            inputs: Object.fromEntries([
                ...flat.map((p) => [p.name, p.inputClass] as const),
                ...nested.map((p) => [p.name, 'cv'] as const),
            ]),
            positional: (schema.positionalArgs ?? []).map((p) => p.name),
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
