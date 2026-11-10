import type { ModuleSchema } from '@modular/core';
import type { ResolvableOutput } from '../../shared/dsl/outputResolutionRule';
import type { SignalSiteTables } from '../../shared/dsl/signalSites';
import { CHAIN_METHODS } from './GraphBuilder';
import { processModuleSchema, qualifiesForDollarChain } from './paramsSchema';
import { signalParamsOf } from './signalParams';
import {
    outputPropertyName,
    qualifiedMultiOutputInterfaceName,
} from './typescriptLibGen';

function resolvableOutputs(schema: ModuleSchema): ResolvableOutput[] {
    const outputs = schema.outputs ?? [];
    const defaultOutput = outputs.find((o) => o.default) ?? outputs[0];
    return outputs.map((o) => ({
        default: o === defaultOutput,
        name: outputPropertyName(o.name),
        outputType: o.outputType,
    }));
}

/**
 * Describe the DSL to the editor's signal-site collector, in the names the
 * generated lib and the runtime use.
 */
export function buildSignalSiteTables(
    schemas: ModuleSchema[],
): SignalSiteTables {
    const tables: SignalSiteTables = {
        chainMethods: [...CHAIN_METHODS],
        dollar: {},
        modules: {},
        roots: {},
    };

    for (const schema of schemas) {
        const processed = processModuleSchema(schema);
        const { flat, nested } = signalParamsOf(schema, processed);
        tables.modules[schema.name] = {
            inputs: Object.fromEntries([
                ...flat.map((p) => [p.name, p.inputClass] as const),
                ...nested.map((p) => [p.name, 'cv'] as const),
            ]),
            positional: (schema.positionalArgs ?? []).map((p) => p.name),
        };
        if (qualifiesForDollarChain(processed)) {
            tables.dollar[schema.name.replace(/^\$/, '')] = schema.name;
        }
        if ((schema.outputs ?? []).length > 1) {
            tables.roots[qualifiedMultiOutputInterfaceName(schema)] =
                resolvableOutputs(schema);
        }
    }
    return tables;
}
