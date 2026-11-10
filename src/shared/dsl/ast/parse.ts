/**
 * The one parse of patch source every static analysis shares. A patch runs
 * as JavaScript, so it is read as JavaScript; parent pointers are set, so any
 * node can walk up to its call or statement.
 *
 * `parseDsl` memoises by source text: the editor asks for the same buffer's
 * tree many times between edits (the control panel on each change, a slider
 * drag on each tick, the quick-fix lightbulb), and those callers share one
 * tree, so they only read it. Text seen once, such as a candidate edit, goes
 * through `parseTransient` instead, so it never evicts the buffer's tree.
 */
import ts from 'typescript';

/** Distinct sources kept: the active buffer and a few recently switched. */
const CACHE_SIZE = 4;

const cache = new Map<string, ts.SourceFile>();

export function parseDsl(source: string): ts.SourceFile {
    const hit = cache.get(source);
    if (hit) {
        // Re-insert so the map's iteration order stays least-recent first.
        cache.delete(source);
        cache.set(source, hit);
        return hit;
    }
    const file = parseTransient(source);
    cache.set(source, file);
    if (cache.size > CACHE_SIZE) {
        cache.delete(cache.keys().next().value!);
    }
    return file;
}

/** A parse of `source` that is not memoised, for text read once. */
export function parseTransient(source: string): ts.SourceFile {
    return ts.createSourceFile(
        'patch.js',
        source,
        ts.ScriptTarget.Latest,
        true,
        ts.ScriptKind.JS,
    );
}
