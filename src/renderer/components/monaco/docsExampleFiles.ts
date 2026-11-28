/**
 * Docs examples are models in the same TypeScript program as the patch
 * buffers, so the worker answers for both. A name the DSL lib declares is
 * shared by every model; an answer about it keeps each example to itself.
 */

/** The folder every docs example's model lives in, apart from any buffer. */
export const DOCS_EXAMPLE_ROOT = 'file:///shoreline-docs/examples/';

export function isDocsExample(fileName: string): boolean {
    return fileName.startsWith(DOCS_EXAMPLE_ROOT);
}

/**
 * Whether a location in `file` belongs in an answer to a request made in
 * `from`: an example sees only itself and the lib, and no buffer sees an
 * example.
 */
export function isVisibleFrom(from: string, file: string): boolean {
    return (
        file === from ||
        file.endsWith('.d.ts') ||
        (!isDocsExample(from) && !isDocsExample(file))
    );
}
