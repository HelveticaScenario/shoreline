/**
 * Compact range/default text for module params and outputs, shared by the
 * Monaco JSDoc generator and the Reference pane so both show the same
 * metadata next to a description.
 */
import { formatHzLiteral, voltsToHz, voltsToNoteName } from './sliderUnits';

/** The signal metadata a param carries in `ModuleSchema.signalParams`. */
export interface SignalMeta {
    signalType: string;
    defaultValue: number;
    minValue?: number;
    maxValue?: number;
}

/** The JSON-schema fields a non-signal param's range and default come from. */
export interface PlainParamSchema {
    type?: unknown;
    format?: unknown;
    minimum?: unknown;
    maximum?: unknown;
    default?: unknown;
}

export interface OutputMeta {
    name: string;
    minValue?: number;
    maxValue?: number;
}

const num = (n: number): string => Number(n.toPrecision(4)).toString();

const hz = (volts: number): string =>
    formatHzLiteral(Number(voltsToHz(volts).toPrecision(3)));

function formatBounds(
    min: number | undefined,
    max: number | undefined,
    show: (n: number) => string,
    symmetric = false,
): string | undefined {
    if (min !== undefined && max !== undefined) {
        return symmetric && min === -max
            ? `±${show(max)}`
            : `${show(min)}–${show(max)}`;
    }
    if (min !== undefined) {
        return `≥ ${show(min)}`;
    }
    if (max !== undefined) {
        return `≤ ${show(max)}`;
    }
    return undefined;
}

/**
 * Range and default of a signal param, e.g. `0–5, default 2.5`, `≥ 0`,
 * `note, default c4`, `gate`. A `required` param has no default to show.
 * `undefined` when there is nothing to add (an unbounded audio input).
 */
export function formatSignalMeta(
    meta: SignalMeta,
    required: boolean,
): string | undefined {
    const parts: string[] = [];
    switch (meta.signalType) {
        case 'gate':
            return 'gate';
        case 'trig':
            return 'trigger';
        case 'pitch': {
            parts.push('note');
            const range = formatBounds(
                meta.minValue,
                meta.maxValue,
                voltsToNoteName,
            );
            if (range) {
                parts.push(range);
            }
            if (!required) {
                parts.push(`default ${voltsToNoteName(meta.defaultValue)}`);
            }
            break;
        }
        case 'frequency': {
            const range = formatBounds(meta.minValue, meta.maxValue, hz);
            if (range) {
                parts.push(range);
            }
            if (!required) {
                parts.push(`default ${hz(meta.defaultValue)}`);
            }
            break;
        }
        default: {
            const range = formatBounds(meta.minValue, meta.maxValue, num, true);
            if (range) {
                parts.push(range);
            }
            if (!required && meta.signalType !== 'audio') {
                parts.push(`default ${num(meta.defaultValue)}`);
            }
        }
    }
    return parts.length > 0 ? parts.join(', ') : undefined;
}

/**
 * Range and default of a plain (non-signal) param from its JSON schema. An
 * unsigned integer's implicit `minimum: 0` is not a declared range and stays
 * hidden unless a maximum accompanies it.
 */
export function formatPlainMeta(schema: PlainParamSchema): string | undefined {
    const parts: string[] = [];
    const min = typeof schema.minimum === 'number' ? schema.minimum : undefined;
    const max = typeof schema.maximum === 'number' ? schema.maximum : undefined;
    const implicitFloor =
        typeof schema.format === 'string' &&
        schema.format.startsWith('uint') &&
        min === 0 &&
        max === undefined;
    const range = implicitFloor ? undefined : formatBounds(min, max, num, true);
    if (range) {
        parts.push(range);
    }
    const d = schema.default;
    if (typeof d === 'number') {
        parts.push(`default ${num(d)}`);
    } else if (typeof d === 'boolean' || typeof d === 'string') {
        parts.push(`default ${String(d)}`);
    }
    return parts.length > 0 ? parts.join(', ') : undefined;
}

/** An output's value range, e.g. `0–5` or `±5`. */
export function formatOutputMeta(output: OutputMeta): string | undefined {
    return formatBounds(output.minValue, output.maxValue, num, true);
}

/** The parts of a module schema a param's metadata is read from. */
export interface ParamMetaSource {
    signalParams: readonly (SignalMeta & { name: string })[];
    paramsSchema: {
        properties?: Readonly<Record<string, unknown>>;
        required?: readonly string[];
    };
}

/** Range/default text for param `name` of a module, from its signal metadata
 *  or, for a plain param, its JSON schema. */
export function paramMeta(
    module: ParamMetaSource,
    name: string,
): string | undefined {
    const required = module.paramsSchema.required?.includes(name) ?? false;
    const signal = module.signalParams.find((p) => p.name === name);
    if (signal) {
        return formatSignalMeta(signal, required);
    }
    const prop = module.paramsSchema.properties?.[name];
    return prop && typeof prop === 'object'
        ? formatPlainMeta(prop as PlainParamSchema)
        : undefined;
}

/** `description (meta)`, or the description alone when there is no meta. */
export function withMeta(
    description: string | undefined,
    meta: string | undefined,
): string {
    const text = description ?? '';
    if (!meta) {
        return text;
    }
    return text ? `${text} (${meta})` : `(${meta})`;
}
