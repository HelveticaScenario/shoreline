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
        signalType: 'control',
    };
}

/** An audio crossfade (`.$m.`, `.pipeMix`): 0 = dry, 5 = wet, 2.5 =
 *  equal. */
const MIX_SPEC = closedControl(0, 5, 2.5);

/** A video crossfade: 0 = the signal, 1 = the result, 0.5 = half way. */
const VIDEO_MIX_SPEC = closedControl(0, 1, 0.5);

/** The leading `mix` argument of a `.$m.` chain call. */
export function mixRef(family: Family): ParamRef {
    return {
        knownDefault: true,
        label: 'mix',
        spec: family === 'video' ? VIDEO_MIX_SPEC : MIX_SPEC,
    };
}

/** Spec of a signal param nothing states a range for: a 0–5 knob with no
 *  usable range, so its unit sliders take their full span. */
const GENERIC_SPEC: ParamSpec = {
    controlMax: 5,
    controlMin: 0,
    defaultValue: 0,
    signalType: 'control',
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
        addHz: {
            offset: { label: 'addHz', module: '$addHz', param: 'offset' },
        },
        amp: {
            factor: { label: 'amp', module: '$scaleAndShift', param: 'scale' },
        },
        amplitude: {
            factor: {
                label: 'amplitude',
                module: '$scaleAndShift',
                param: 'scale',
            },
        },
        exp: { exponent: { label: 'exp', module: '$curve', param: 'exp' } },
        // gain's level is perceptual: 5 = unity, 0 = silence — narrower than
        // scale's control range.
        gain: {
            level: {
                label: 'gain',
                module: '$scaleAndShift',
                param: 'scale',
                range: { max: 5, min: 0 },
            },
        },
        mulHz: {
            factor: { label: 'mulHz', module: '$mulHz', param: 'factor' },
        },
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
        shift: {
            offset: {
                label: 'shift',
                module: '$scaleAndShift',
                param: 'shift',
            },
        },
    },
    video: {
        pipeMix: { mix: { spec: VIDEO_MIX_SPEC } },
    },
};

export function paramSpec(
    schema: SchemaEntry,
    name: string,
): ParamSpec | undefined {
    return schema.signalParams.find((p) => p.name === name);
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
    const spec = module && paramSpec(module, source.param);
    if (!spec) {
        return null;
    }
    return {
        knownDefault: true,
        label: source.label ?? name,
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
