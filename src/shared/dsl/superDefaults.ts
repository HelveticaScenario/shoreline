import type { ModuleSchema } from '@modular/core';

/** What `$super` uses for each option left unset. */
export interface SuperDefaults {
    /** Copies per note: `$supersaw`'s `unison`. */
    unison: number;
    /** Semitones between the lowest and highest copy: `$unison`'s `spread`. */
    detune: number;
    /** How far the copies fan out: `$stereoFold`'s `width`. */
    width: number;
}

/**
 * `$super`'s defaults, read from the modules it stands in for so the two
 * can never disagree. Null when any of them is missing from the schemas.
 */
export function superDefaults(schemas: ModuleSchema[]): SuperDefaults | null {
    const find = (name: string) => schemas.find((s) => s.name === name);
    const signalDefault = (module: string, param: string) =>
        find(module)?.signalParams.find((p) => p.name === param)?.defaultValue;
    const unison = (
        find('$supersaw')?.paramsSchema as
            | { properties?: Record<string, { default?: unknown }> }
            | undefined
    )?.properties?.unison?.default;
    const detune = signalDefault('$unison', 'spread');
    const width = signalDefault('$stereoFold', 'width');
    return typeof unison === 'number' &&
        typeof detune === 'number' &&
        typeof width === 'number'
        ? { detune, unison, width }
        : null;
}
