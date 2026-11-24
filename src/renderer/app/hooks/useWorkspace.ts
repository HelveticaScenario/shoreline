import { useEffect, useState } from 'react';
import type { FileTreeEntry } from '../../../shared/ipcTypes';
import electronAPI from '../../electronAPI';

/** The open workspace folder and its file tree, kept current with the
 *  folder's changes on disk. */
export function useWorkspace() {
    const [root, setRoot] = useState<string | null>(null);
    const [fileTree, setFileTree] = useState<FileTreeEntry[]>([]);

    const refreshFileTree = async () => {
        try {
            setFileTree(await electronAPI.filesystem.listFiles());
        } catch (error) {
            console.error('Failed to refresh file tree:', error);
        }
    };

    useEffect(() => {
        electronAPI.filesystem
            .getWorkspace()
            .then((workspace) => {
                if (workspace) {
                    setRoot(workspace.path);
                    void refreshFileTree();
                }
            })
            .catch((err) => {
                console.error('Failed to load workspace:', err);
            });
        return electronAPI.filesystem.onTreeChange(
            () => void refreshFileTree(),
        );
    }, []);

    /** Ask the user for a folder and open it; cancelling keeps the current
     *  one. */
    const choose = async () => {
        const workspace = await electronAPI.filesystem.selectWorkspace();
        if (workspace) {
            setRoot(workspace.path);
            await refreshFileTree();
        }
    };

    return { choose, fileTree, refreshFileTree, root };
}
