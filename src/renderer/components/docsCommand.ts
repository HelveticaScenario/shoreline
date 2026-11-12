/** A symbol to reveal in the Docs panel, such as the target of a JSDoc link. */
export interface DocsTarget {
    symbolType: 'type' | 'module' | 'namespace';
    name: string;
}

/** Brings the docs forward, revealing a {@link DocsTarget} when given one. */
export const SHOW_DOCS_COMMAND = 'shoreline.showDocs';

const SYMBOL_TYPES: ReadonlySet<string> = new Set([
    'type',
    'module',
    'namespace',
]);

/** Narrows a command argument, which a user keybinding can set to any value. */
export function toDocsTarget(value: unknown): DocsTarget | null {
    if (typeof value !== 'object' || value === null) {
        return null;
    }
    const { name, symbolType } = value as Record<string, unknown>;
    if (
        typeof name !== 'string' ||
        typeof symbolType !== 'string' ||
        !SYMBOL_TYPES.has(symbolType)
    ) {
        return null;
    }
    return { name, symbolType: symbolType as DocsTarget['symbolType'] };
}
