import * as fs from 'fs';
import * as path from 'path';

/** Whether `file` is `folder` or lies inside it. */
export function isWithin(folder: string, file: string): boolean {
    return file === folder || file.startsWith(folder + path.sep);
}

/**
 * Resolve a caller-supplied file path to an absolute path, enforcing
 * workspace containment. Relative paths resolve against the workspace root;
 * absolute paths must lie inside it, either as written or, once links are
 * followed, inside the workspace folder or one of its `linkedFolders` (real
 * target path to the workspace-relative path of its link), in which case the
 * result names the file through the workspace. Returns null when there is no
 * workspace or the resolved path escapes it.
 */
export function resolveWorkspacePath(
    workspaceRoot: string | null,
    filePath: string,
    linkedFolders: ReadonlyMap<string, string> = new Map(),
): string | null {
    if (!workspaceRoot) {
        return null;
    }
    // path.resolve normalizes away any embedded '..' segments before the
    // containment check below.
    const resolved = path.isAbsolute(filePath)
        ? path.resolve(filePath)
        : path.resolve(workspaceRoot, filePath);

    const relative = path.relative(workspaceRoot, resolved);
    const escapesWorkspace =
        relative === '..' ||
        relative.startsWith(`..${path.sep}`) ||
        path.isAbsolute(relative);
    if (!escapesWorkspace) {
        return resolved;
    }
    return path.isAbsolute(filePath)
        ? throughLinks(workspaceRoot, resolved, linkedFolders)
        : null;
}

/** `file`, a path outside the workspace as written, named through the
 *  workspace folder or the link to the folder it really lies in. */
function throughLinks(
    workspaceRoot: string,
    file: string,
    linkedFolders: ReadonlyMap<string, string>,
): string | null {
    let realRoot: string;
    try {
        realRoot = fs.realpathSync(workspaceRoot);
    } catch {
        return null;
    }
    // The file itself may not exist yet, as when saving to a new name.
    let real = file;
    try {
        real = path.join(
            fs.realpathSync(path.dirname(file)),
            path.basename(file),
        );
    } catch {
        // An unreadable folder is matched as written.
    }
    for (const [target, link] of [[realRoot, ''] as const, ...linkedFolders]) {
        if (isWithin(target, real)) {
            return path.join(workspaceRoot, link, path.relative(target, real));
        }
    }
    return null;
}
