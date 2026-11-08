/**
 * The one parse of patch source every static analysis shares. A patch runs
 * as JavaScript, so it is read as JavaScript; parent pointers are set, so any
 * node can walk up to its call or statement.
 *
 * Results are memoised by source text: the editor asks for the same buffer's
 * tree many times between edits (the control panel on each change, a slider
 * drag on each tick, the quick-fix lightbulb). A tree is shared between those
 * callers, so it must never be mutated.
 */
import ts from 'typescript';

/** Distinct sources kept: the buffer being edited plus a few transient
 *  texts (a quick fix's candidate edits) parsed between its edits. */
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
    const file = ts.createSourceFile(
        'patch.js',
        source,
        ts.ScriptTarget.Latest,
        true,
        ts.ScriptKind.JS,
    );
    cache.set(source, file);
    if (cache.size > CACHE_SIZE) {
        cache.delete(cache.keys().next().value!);
    }
    return file;
}
