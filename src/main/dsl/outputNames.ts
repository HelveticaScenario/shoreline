/**
 * The name an output goes by in the DSL: its schema name in camelCase. A
 * module has it behind an underscore (`seq._gate`).
 */
export function outputName(name: string): string {
    return name.replace(/_([a-z])/g, (_, letter: string) =>
        letter.toUpperCase(),
    );
}
