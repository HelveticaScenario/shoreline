/**
 * The renderer's signal that the workspace's files are being re-read: on
 * every change the watcher reports and on every explicit refresh, so what
 * caches the files can follow the explorer even when the watcher is down.
 */
const listeners = new Set<() => void>();

/** Calls `callback` on every re-read; returns the unsubscribe. */
export function onWorkspaceRefresh(callback: () => void): () => void {
    listeners.add(callback);
    return () => listeners.delete(callback);
}

export function announceWorkspaceRefresh(): void {
    for (const listener of listeners) listener();
}
