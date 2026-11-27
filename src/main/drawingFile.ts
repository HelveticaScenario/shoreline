import * as fs from 'fs';
import * as path from 'path';
import { BUILTIN_PREFIX } from '../shared/builtinAssets';
import { workspaceFile } from './mediaProtocol';

/** Drawings larger than this are refused, since the whole text goes into the patch. */
const MAX_DRAWING_BYTES = 4 * 1024 * 1024;

/**
 * The text of the SVG file at `rooted`, a `builtin/` path or a `/`-rooted
 * workspace path already checked by `assetPath`; throws an error naming the
 * path when the file cannot be read.
 */
export function readDrawingFile(
    rooted: string,
    workspaceRoot: string | null,
    builtinDir: string,
): string {
    const file = rooted.startsWith(BUILTIN_PREFIX)
        ? path.join(builtinDir, rooted.slice(BUILTIN_PREFIX.length))
        : workspaceFile(workspaceRoot, rooted.slice(1));
    if (file === null) {
        throw new Error(`"${rooted}": no workspace folder is open`);
    }
    let size: number;
    try {
        size = fs.statSync(file).size;
    } catch {
        throw new Error(`"${rooted}": no such file`);
    }
    if (size > MAX_DRAWING_BYTES) {
        throw new Error(
            `"${rooted}": drawings are limited to ${MAX_DRAWING_BYTES / 1024 / 1024} MB`,
        );
    }
    return fs.readFileSync(file, 'utf8');
}
