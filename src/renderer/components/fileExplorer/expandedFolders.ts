import type { FileTreeEntry } from '../../../shared/ipcTypes';

const EXPANDED_STORAGE_KEY = 'modular_expanded_folders';

function directoryPaths(entries: FileTreeEntry[], into = new Set<string>()) {
    for (const entry of entries) {
        if (entry.type === 'directory') {
            into.add(entry.path);
            if (entry.children) directoryPaths(entry.children, into);
        }
    }
    return into;
}

/** The folders left expanded at the last launch, as a tree open state. */
export function readExpandedFolders(): Record<string, boolean> {
    try {
        const raw = window.localStorage.getItem(EXPANDED_STORAGE_KEY);
        if (raw) {
            return Object.fromEntries(
                (JSON.parse(raw) as string[]).map((path) => [path, true]),
            );
        }
    } catch {}
    return {};
}

/**
 * Remembers the expanded folders of `openState` for the next launch. Folders
 * missing from a loaded `fileTree` are dropped; an empty tree keeps them all.
 */
export function storeExpandedFolders(
    openState: Record<string, boolean>,
    fileTree: FileTreeEntry[],
) {
    const present = directoryPaths(fileTree);
    const expanded = Object.keys(openState).filter(
        (path) => openState[path] && (present.size === 0 || present.has(path)),
    );
    try {
        window.localStorage.setItem(
            EXPANDED_STORAGE_KEY,
            JSON.stringify(expanded),
        );
    } catch {}
}
