import type { ModuleSchema } from '@modular/core';
import {
    paramTypeClass,
    type OutputClass,
} from '../../shared/dsl/outputResolutionRule';
import type { ProcessedModuleSchema } from './paramsSchema';
import { buildSignalMapper, type SignalMapper } from './signalSchemaMapper';

/** Where a module's params take signals. */
export interface SignalParams {
    /** Params that are a signal, or an array of signals, themselves. */
    flat: { name: string; inputClass: OutputClass }[];
    /** Params that nest signals inside arrays or tuples; each reads `cv`. */
    nested: { name: string; mapper: SignalMapper }[];
}

/** The signal-taking params of a module. */
export function signalParamsOf(
    schema: ModuleSchema,
    processed: ProcessedModuleSchema,
): SignalParams {
    const flat = processed.params
        .filter(
            (p) =>
                p.kind === 'signal' ||
                p.kind === 'polySignal' ||
                p.isPolySignalInput ||
                p.isLaneSumInput ||
                p.isScaleSignalInput,
        )
        .map((p) => ({
            inputClass: paramTypeClass(p.paramType),
            name: p.name,
        }));
    const flatNames = new Set(flat.map((p) => p.name));

    const paramsSchema = schema.paramsSchema as {
        properties?: Record<string, Record<string, unknown>>;
        $defs?: Record<string, Record<string, unknown>>;
    };
    const nested = processed.params
        .filter((p) => !flatNames.has(p.name))
        .flatMap((p) => {
            const property = paramsSchema.properties?.[p.name];
            const mapper = property
                ? buildSignalMapper(property, paramsSchema.$defs)
                : null;
            return mapper ? [{ mapper, name: p.name }] : [];
        });

    return { flat, nested };
}
