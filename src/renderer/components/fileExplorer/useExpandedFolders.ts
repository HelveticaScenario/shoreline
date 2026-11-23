import { useEffect, useState } from 'react';
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

function readStoredPaths(): Set<string> {
    try {
        const raw = window.localStorage.getItem(EXPANDED_STORAGE_KEY);
        if (raw) return new Set(JSON.parse(raw) as string[]);
    } catch {}
    return new Set();
}

/**
 * The expanded folders of `fileTree`, remembered in localStorage across
 * launches, and a function that expands or collapses one. Folders missing from
 * a loaded tree are dropped; an empty tree keeps them all.
 */
export function useExpandedFolders(
    fileTree: FileTreeEntry[],
): [Set<string>, (path: string) => void] {
    const [storedPaths, setStoredPaths] = useState(readStoredPaths);

    const present = directoryPaths(fileTree);
    const expandedPaths =
        present.size === 0
            ? storedPaths
            : new Set([...storedPaths].filter((path) => present.has(path)));

    const serialized = JSON.stringify([...expandedPaths]);
    useEffect(() => {
        try {
            window.localStorage.setItem(EXPANDED_STORAGE_KEY, serialized);
        } catch {}
    }, [serialized]);

    const toggleFolder = (path: string) => {
        setStoredPaths((prev) => {
            const next = new Set(prev);
            if (next.has(path)) next.delete(path);
            else next.add(path);
            return next;
        });
    };

    return [expandedPaths, toggleFolder];
}
