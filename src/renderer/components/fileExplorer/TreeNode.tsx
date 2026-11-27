import {
    AudioLines,
    Film,
    Folder,
    FolderOpen,
    Image,
    Parentheses,
    PenLine,
    type LucideIcon,
} from 'lucide-react';
import type { MouseEvent } from 'react';
import type { FileTreeEntry } from '../../../shared/ipcTypes';
import { toAbsoluteWorkspacePath } from '../../app/buffers';
import { WORKSPACE_PATH_MIME } from '../monaco/workspacePathCompletion';
import { RenameInput } from './RenameInput';

const FILE_ICONS: Record<NonNullable<FileTreeEntry['fileType']>, LucideIcon> = {
    patch: Parentheses,
    wav: AudioLines,
    image: Image,
    video: Film,
    drawing: PenLine,
};

interface TreeNodeProps {
    entry: FileTreeEntry;
    workspaceRoot: string;
    onOpenFile: (relPath: string, options?: { preview?: boolean }) => void;
    onContextMenu: (e: MouseEvent, entry: FileTreeEntry) => void;
    renamingPath: string | null;
    onRenameCommit: (path: string, newName: string) => void;
    onRenameCancel: () => void;
    expandedPaths: Set<string>;
    onToggleFolder: (path: string) => void;
}

/** A file or folder of the workspace tree, with the folder's children under it. */
export function TreeNode(props: TreeNodeProps) {
    const { entry, onContextMenu } = props;
    if (entry.type === 'file') {
        return <FileNode {...props} />;
    }

    const expanded = props.expandedPaths.has(entry.path);
    const isRenaming =
        props.renamingPath ===
        toAbsoluteWorkspacePath(props.workspaceRoot, entry.path);
    return (
        <li className="tree-folder">
            <div
                className="folder-header"
                onClick={() => !isRenaming && props.onToggleFolder(entry.path)}
                onContextMenu={(e) => onContextMenu(e, entry)}
            >
                {expanded ? (
                    <FolderOpen className="folder-icon" aria-hidden />
                ) : (
                    <Folder className="folder-icon" aria-hidden />
                )}
                <span className="folder-name">{entry.name}</span>
            </div>
            {expanded && entry.children && entry.children.length > 0 && (
                <ul className="tree-children">
                    {entry.children.map((child) => (
                        <TreeNode {...props} key={child.path} entry={child} />
                    ))}
                </ul>
            )}
        </li>
    );
}

function FileNode({
    entry,
    workspaceRoot,
    onOpenFile,
    onContextMenu,
    renamingPath,
    onRenameCommit,
    onRenameCancel,
}: TreeNodeProps) {
    // FileTreeEntry paths are workspace-relative; renamingPath (like buffer
    // identity) is an absolute path, so resolve before comparing.
    const absolutePath = toAbsoluteWorkspacePath(workspaceRoot, entry.path);
    const isRenaming = renamingPath === absolutePath;
    const isAsset = entry.fileType !== undefined && entry.fileType !== 'patch';
    const FileIcon = FILE_ICONS[entry.fileType ?? 'patch'];

    const open = (preview: boolean) => {
        if (!isRenaming && !isAsset) {
            onOpenFile(entry.path, { preview });
        }
    };

    return (
        <li
            className={`tree-file${isAsset ? ' tree-file-asset' : ''}`}
            onClick={() => open(true)}
            onDoubleClick={() => open(false)}
            onContextMenu={(e) => !isAsset && onContextMenu(e, entry)}
            draggable={isAsset && !isRenaming}
            title={
                isAsset
                    ? `Drag into the editor to use /${entry.path}`
                    : undefined
            }
            onDragStart={(e) => {
                e.dataTransfer.setData(WORKSPACE_PATH_MIME, `/${entry.path}`);
                e.dataTransfer.effectAllowed = 'copy';
            }}
        >
            <FileIcon className="file-icon" aria-hidden />
            {isRenaming ? (
                <RenameInput
                    name={entry.name}
                    onCommit={(newName) =>
                        onRenameCommit(absolutePath, newName)
                    }
                    onCancel={onRenameCancel}
                />
            ) : (
                <span className="file-name">{entry.name}</span>
            )}
        </li>
    );
}
