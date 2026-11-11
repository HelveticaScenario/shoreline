import type { ModuleSchema } from '@modular/core';
import type { SignalSiteTables } from '../../shared/dsl/signalSites';
import { CHAIN_METHODS } from './chainMethods';
import { resolvableOutputs } from './outputResolution';
import { processModuleSchema, qualifiesForDollarChain } from './paramsSchema';
import { signalParamsOf } from './signalParams';
import {
    qualifiedMultiOutputInterfaceName,
    sanitizeOutputName,
} from './typescriptLibGen';

/**
 * Describe the DSL to the editor's signal-site collector, in the names the
 * generated lib and the runtime use.
 */
export function buildSignalSiteTables(
    schemas: ModuleSchema[],
): SignalSiteTables {
    const tables: SignalSiteTables = {
        chainMethods: CHAIN_METHODS,
        // `$delay(input, time, { feedback })` is a helper, not a module.
        modules: {
            $delay: {
                chainable: true,
                inputs: { feedback: 'cv', input: 'cv', time: 'cv' },
                positional: ['input', 'time'],
            },
        },
        roots: {},
    };

    for (const schema of schemas) {
        const processed = processModuleSchema(schema);
        const { flat, nested } = signalParamsOf(schema, processed);
        tables.modules[schema.name] = {
            chainable: qualifiesForDollarChain(processed),
            inputs: Object.fromEntries([
                ...flat.map((p) => [p.name, p.inputClass] as const),
                ...nested.map((p) => [p.name, 'cv'] as const),
            ]),
            positional: (schema.positionalArgs ?? []).map((p) => p.name),
        };
        if ((schema.outputs ?? []).length > 1) {
            tables.roots[qualifiedMultiOutputInterfaceName(schema)] =
                resolvableOutputs(schema, sanitizeOutputName);
        }
    }
    return tables;
}
