/**
 * Which output of a multi-output module a bare reference to it stands for,
 * given the input it is plugged into. Shared by the DSL runtime and the
 * editor's hint collector so the two always agree.
 */

/** The `outputType` of a module output. */
export type OutputClass = 'cv' | 'gate' | 'trig';

/** The fields of an output schema the rule reads. */
export interface ResolvableOutput {
    name: string;
    outputType: string;
    default?: boolean;
}

/**
 * The output class an input reads. Every continuous `paramType` (audio,
 * control, pitch, frequency) reads `cv`.
 */
export function paramTypeClass(paramType: string | undefined): OutputClass {
    return paramType === 'gate' || paramType === 'trig' ? paramType : 'cv';
}

/**
 * Pick the output an input of class `inputClass` takes from a module's
 * `outputs`: the default when it has that class, else the first of that class
 * in declaration order, else the first gate for a trig input or trig for a
 * gate input, else the default. Always returns an output of the module.
 */
export function chooseOutput(
    outputs: readonly ResolvableOutput[],
    inputClass: OutputClass,
): string {
    const fallback = outputs.find((o) => o.default) ?? outputs[0];
    if (fallback.outputType === inputClass) {
        return fallback.name;
    }
    const sameClass = outputs.find((o) => o.outputType === inputClass);
    if (sameClass) {
        return sameClass.name;
    }
    if (inputClass !== 'cv') {
        const neighbour = inputClass === 'trig' ? 'gate' : 'trig';
        const event = outputs.find((o) => o.outputType === neighbour);
        if (event) {
            return event.name;
        }
    }
    return fallback.name;
}

/** The slot of a chain method's receiver. */
export const RECEIVER_SLOT = 'receiver';

/** The slot of the mix amount of `.pipeMix(fn, mix)`. */
export const MIX_SLOT = 'mix';

/** The slot of element `index` of the array at `slot`. */
export function elementSlot(slot: string, index: number): string {
    return `${slot}.${index}`;
}

/** The slot of the array that element slot `slot` belongs to, if it is one. */
export function arraySlotOf(slot: string): string | undefined {
    const dot = slot.lastIndexOf('.');
    return dot !== -1 && /^\d+$/.test(slot.slice(dot + 1))
        ? slot.slice(0, dot)
        : undefined;
}

/** The slot of a chain method's argument `index`. */
export function argumentSlot(index: number): string {
    return String(index);
}

/** The slot of field `key` of the options object at argument `index`. */
export function optionSlot(index: number, key: string): string {
    return `${index}.${key}`;
}

/**
 * One contextual resolution made while a patch ran. `line`/`column` are the
 * call site as V8 reports it (first-line columns carry
 * `FIRST_LINE_COLUMN_OFFSET`). `slot` names the value within that call: for a
 * module call or `$delay`, the param name or the path into a param that
 * nests signals; for a chain method, `RECEIVER_SLOT`, an `argumentSlot` or an
 * `optionSlot`; `MIX_SLOT` for the mix amount of `.pipeMix`; any of them through
 * `elementSlot` for an element of an array. `output` is the property name of
 * the chosen output.
 */
export interface OutputResolutionRecord {
    line: number;
    column: number;
    slot: string;
    output: string;
}
