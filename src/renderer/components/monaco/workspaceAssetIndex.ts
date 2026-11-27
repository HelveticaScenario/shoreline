/**
 * The workspace's path completions, read once and kept until the workspace's
 * files change, so completing never waits on the main process.
 */
import type { SyxFileVoices } from '../../../shared/dx7Voice';
import type { FileTreeEntry } from '../../../shared/ipcTypes';
import {
    assetPaths,
    type PathSuggestion,
    workspaceSuggestions,
} from './workspacePathCompletion';

export interface WorkspaceAssetSource {
    listFiles: () => Promise<FileTreeEntry[]>;
    /** The voice names of each readable `.syx` file among `paths`. */
    listSyxVoices: (paths: string[]) => Promise<SyxFileVoices[]>;
    /** Calls `callback` when the workspace's files change; returns the unsubscribe. */
    onTreeChange: (callback: () => void) => () => void;
}

export function createWorkspaceAssetIndex(source: WorkspaceAssetSource): {
    suggestions: () => Promise<readonly PathSuggestion[]>;
    dispose: () => void;
} {
    let cached: Promise<readonly PathSuggestion[]> | null = null;
    const read = async (): Promise<readonly PathSuggestion[]> => {
        const assets = assetPaths(await source.listFiles());
        const syxPaths = assets
            .filter((asset) => asset.kind === 'syx')
            .map((asset) => asset.path);
        const voices =
            syxPaths.length > 0 ? await source.listSyxVoices(syxPaths) : [];
        return workspaceSuggestions(assets, voices);
    };
    const dispose = source.onTreeChange(() => {
        cached = null;
    });
    return {
        suggestions: () => {
            if (!cached) {
                const reading = read().catch((error: unknown) => {
                    console.error(
                        '[path completion] failed to list files',
                        error,
                    );
                    // A failed read is not kept, so the next request tries again.
                    if (cached === reading) cached = null;
                    return [];
                });
                cached = reading;
            }
            return cached;
        },
        dispose,
    };
}
