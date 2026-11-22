import * as fs from 'fs';
import * as path from 'path';
import { assetKind } from '../shared/assetFiles';
import type { FileTreeEntry } from '../shared/ipcTypes';
import { isPatchPath } from '../shared/patchFiles';

export interface WorkspaceTree {
    /** The workspace's patches and loadable assets, folder by folder. */
    entries: FileTreeEntry[];
    /**
     * Each linked folder whose target lies outside the workspace folder, by
     * the target's real path, to the workspace-relative path of its first
     * link.
     */
    linkedFolders: Map<string, string>;
}

/** The explorer's file type for a patch or an asset a patch can load, or undefined for any other file. */
function explorerFileType(filename: string): FileTreeEntry['fileType'] {
    if (isPatchPath(filename)) {
        return 'patch';
    }
    return assetKind(filename);
}

/** Whether `file` is `folder` or lies inside it; both are real paths. */
export function isWithin(folder: string, file: string): boolean {
    return file === folder || file.startsWith(folder + path.sep);
}

/**
 * Read the workspace folder, following links to files and folders wherever
 * they point. Hidden files and `node_modules` are skipped, as are broken
 * links and a link back into a folder it sits in, which would never end.
 */
export function readWorkspaceTree(root: string): WorkspaceTree {
    const linkedFolders = new Map<string, string>();
    let realRoot: string;
    try {
        realRoot = fs.realpathSync(root);
    } catch (error) {
        console.error('Error reading directory:', root, error);
        return { entries: [], linkedFolders };
    }

    const read = (
        dirPath: string,
        relativePath: string,
        ancestors: ReadonlySet<string>,
    ): FileTreeEntry[] => {
        const entries: FileTreeEntry[] = [];
        let items: fs.Dirent[];
        try {
            items = fs.readdirSync(dirPath, { withFileTypes: true });
        } catch (error) {
            console.error('Error reading directory:', dirPath, error);
            return entries;
        }

        for (const item of items) {
            if (item.name.startsWith('.') || item.name === 'node_modules') {
                continue;
            }
            const itemRelativePath = relativePath
                ? `${relativePath}/${item.name}`
                : item.name;
            const itemAbsolutePath = path.join(dirPath, item.name);

            let isDirectory = item.isDirectory();
            let isFile = item.isFile();
            if (item.isSymbolicLink()) {
                try {
                    const target = fs.statSync(itemAbsolutePath);
                    isDirectory = target.isDirectory();
                    isFile = target.isFile();
                } catch {
                    continue;
                }
            }

            if (isDirectory) {
                let real: string;
                try {
                    real = fs.realpathSync(itemAbsolutePath);
                } catch {
                    continue;
                }
                if (ancestors.has(real)) {
                    continue;
                }
                if (
                    item.isSymbolicLink() &&
                    !isWithin(realRoot, real) &&
                    !linkedFolders.has(real)
                ) {
                    linkedFolders.set(real, itemRelativePath);
                }
                const children = read(
                    itemAbsolutePath,
                    itemRelativePath,
                    new Set(ancestors).add(real),
                );
                if (children.length > 0) {
                    entries.push({
                        children,
                        name: item.name,
                        path: itemRelativePath,
                        type: 'directory',
                    });
                }
            } else if (isFile && explorerFileType(item.name)) {
                entries.push({
                    fileType: explorerFileType(item.name),
                    name: item.name,
                    path: itemRelativePath,
                    type: 'file',
                });
            }
        }

        // Directories first, then files, alphabetically.
        return entries.sort((a, b) => {
            if (a.type !== b.type) {
                return a.type === 'directory' ? -1 : 1;
            }
            return a.name.localeCompare(b.name);
        });
    };

    return {
        entries: read(root, '', new Set([realRoot])),
        linkedFolders,
    };
}
