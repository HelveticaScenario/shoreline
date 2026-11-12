/**
 * Ranges and defaults for the controls an API param gets: module params
 * from their schema, other API params from API_CONTROLS, then from what the
 * DSL lib's JSDoc states, over a generic 0–5 knob.
 */
import type { Schemas } from '../../shared/dsl/schemaTypeResolver';
import type { Family } from './apiSignatures';
import type { DocHints } from './apiTypeReader';
import type { ParamSpec } from './paramControls';

export type SchemaEntry = Schemas[number];

/** A param a control can drive, with the label base its control gets. */
export interface ParamRef {
    label: string;
    spec: ParamSpec;
    /** Whether `spec.defaultValue` is the value the call uses while the
     *  param is unset, so adding a control there keeps the sound. */
    knownDefault: boolean;
}

/** A control whose slider spans exactly its usable range. */
function closedControl(
    min: number,
    max: number,
    defaultValue: number,
): ParamSpec {
    return {
        controlMax: max,
        controlMin: min,
        defaultValue,
        maxValue: max,
        minValue: min,
        paramType: 'control',
    };
}

/** An audio crossfade (`.wet`, `.pipeMix`): 0 = dry, 5 = wet, 2.5 =
 *  equal. */
const MIX_SPEC = closedControl(0, 5, 2.5);

/** A video crossfade: 0 = the signal, 1 = the result, 0.5 = half way. */
const VIDEO_MIX_SPEC = closedControl(0, 1, 0.5);

/** Spec of a signal param nothing states a range for: a 0–5 knob with no
 *  usable range, so its unit sliders take their full span. */
const GENERIC_SPEC: ParamSpec = {
    controlMax: 5,
    controlMin: 0,
    defaultValue: 0,
    paramType: 'control',
};

const LEVEL = closedControl(0, 5, 5);

export type ControlSource =
    | { spec: ParamSpec; label?: string }
    | {
          module: string;
          param: string;
          label?: string;
          range?: { min: number; max: number };
      };

/**
 * Controls for API params whose range or default the lib's JSDoc does not
 * state in a form docHints reads, keyed by family, callee (method name or
 * dotted function name), then param or option-member name. The
 * single-argument audio sugar methods wrap a utility module's param, whose
 * schema supplies the spec, and label their control with the method name.
 */
export const API_CONTROLS: Record<
    Family,
    Record<string, Record<string, ControlSource>>
> = {
    audio: {
        $setOutputGain: { gain: { spec: { ...LEVEL, defaultValue: 2.5 } } },
        out: {
            gain: { spec: LEVEL },
            pan: { spec: closedControl(-5, 5, 0) },
            width: { spec: { ...LEVEL, defaultValue: 0 } },
        },
        outMono: {
            gain: { spec: LEVEL },
            gainOrOptions: { label: 'gain', spec: LEVEL },
        },
        pipeMix: { mix: { spec: MIX_SPEC } },
        send: { gain: { spec: LEVEL } },
        wet: { mix: { spec: MIX_SPEC } },
    },
    video: {
        pipeMix: { mix: { spec: VIDEO_MIX_SPEC } },
        wet: { mix: { spec: VIDEO_MIX_SPEC } },
    },
};

/** A control for `name` as a `ParamRef`. Its default is known for a
 *  required param (no unset value to keep), a gate or trigger (unset reads
 *  low) and a declared default; an optional input without one (a `rise`
 *  that follows `fall`) does not read 0 while unset. */
export function schemaParamRef(
    schema: SchemaEntry,
    name: string,
    label: string,
): ParamRef | null {
    const spec = paramSpec(schema, name);
    if (!spec) {
        return null;
    }
    const signal: { paramType: string; defaultValue?: number } | undefined =
        schema.signalParams.find((s) => s.name === name);
    const knownDefault =
        (schema.paramsSchema.required ?? []).includes(name) ||
        signal?.paramType === 'gate' ||
        signal?.paramType === 'trig' ||
        signal?.defaultValue !== undefined;
    return { knownDefault, label, spec };
}

export function paramSpec(
    schema: SchemaEntry,
    name: string,
): ParamSpec | undefined {
    const p = schema.signalParams.find((s) => s.name === name);
    // A param without a declared default starts its control at 0.
    if (!p) {
        return undefined;
    }
    const meta: Omit<ParamSpec, 'defaultValue'> & { defaultValue?: number } = p;
    return { ...meta, defaultValue: meta.defaultValue ?? 0 };
}

export function controlRef(
    name: string,
    source: ControlSource,
    byName: ReadonlyMap<string, SchemaEntry>,
): ParamRef | null {
    if ('spec' in source) {
        return {
            knownDefault: true,
            label: source.label ?? name,
            spec: source.spec,
        };
    }
    const module = byName.get(source.module);
    const ref =
        module && schemaParamRef(module, source.param, source.label ?? name);
    if (!ref) {
        return null;
    }
    const { spec } = ref;
    return {
        ...ref,
        spec: {
            ...spec,
            controlMax: source.range?.max ?? spec.controlMax,
            controlMin: source.range?.min ?? spec.controlMin,
        },
    };
}

/** A param spec from doc-stated hints, over the generic 0–5 knob. */
export function hintedRef(name: string, hints: DocHints): ParamRef {
    const spec: ParamSpec = hints.range
        ? closedControl(hints.range.min, hints.range.max, hints.range.min)
        : { ...GENERIC_SPEC };
    if (hints.defaultValue !== undefined) {
        spec.defaultValue = hints.defaultValue;
    }
    return {
        knownDefault: hints.defaultValue !== undefined,
        label: name,
        spec,
    };
}
