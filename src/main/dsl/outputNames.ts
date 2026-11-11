import { getReservedOutputNames } from '@modular/core';

/**
 * Property names taken by the methods and properties of ModuleOutput,
 * Collection and CollectionWithRange. Single source of truth:
 * `crates/reserved_output_names.rs`.
 */
const RESERVED_OUTPUT_NAMES: ReadonlySet<string> = new Set(
    getReservedOutputNames(),
);

/**
 * The property an output has on its module: its name in camelCase, with an
 * underscore appended when that is reserved.
 */
export function sanitizeOutputName(name: string): string {
    const camelName = name.replace(/_([a-z])/g, (_, letter: string) =>
        letter.toUpperCase(),
    );
    return RESERVED_OUTPUT_NAMES.has(camelName) ? `${camelName}_` : camelName;
}
