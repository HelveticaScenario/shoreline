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

/**
 * One contextual resolution made while a patch ran. `line`/`column` are the
 * call site as V8 reports it (first-line columns carry
 * `FIRST_LINE_COLUMN_OFFSET`). `slot` names the value within that call: a
 * param name for a module call, with `.i` for an array element, or `receiver`
 * or an argument index for a chain method. `output` is the property name of
 * the chosen output.
 */
export interface OutputResolutionRecord {
    line: number;
    column: number;
    slot: string;
    output: string;
}
