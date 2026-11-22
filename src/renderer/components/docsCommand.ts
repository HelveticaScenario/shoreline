const SYMBOL_TYPES = ['type', 'module', 'namespace', 'global'] as const;

type SymbolType = (typeof SYMBOL_TYPES)[number];

/** A symbol to reveal in the Docs panel, such as the target of a JSDoc link. */
export interface DocsTarget {
    symbolType: SymbolType;
    name: string;
}

/** Brings the docs forward, revealing a {@link DocsTarget} when given one. */
export const SHOW_DOCS_COMMAND = 'shoreline.showDocs';

const isSymbolType = (value: unknown): value is SymbolType =>
    SYMBOL_TYPES.some((t) => t === value);

/** Narrows a command argument, which a user keybinding can set to any value. */
export function toDocsTarget(value: unknown): DocsTarget | null {
    if (typeof value !== 'object' || value === null) {
        return null;
    }
    const { name, symbolType } = value as Record<string, unknown>;
    if (typeof name !== 'string' || !isSymbolType(symbolType)) {
        return null;
    }
    return { name, symbolType };
}
