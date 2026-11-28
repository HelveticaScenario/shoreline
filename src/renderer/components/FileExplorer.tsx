import { type MouseEvent, type ReactNode, useEffect, useRef } from 'react';
import './FileExplorer.css';
import type { FileTreeEntry } from '../../shared/ipcTypes';
import type { EditorBuffer } from '../types/editor';
import { getBufferId, toAbsoluteWorkspacePath } from '../app/buffers';
import electronAPI from '../electronAPI';
import { bindFileExplorerFocus } from '../keybindings/contextKeyBootstrap';
import { BufferItem } from './fileExplorer/BufferItem';
import { WorkspaceTree } from './fileExplorer/WorkspaceTree';

interface FileExplorerProps {
    workspaceRoot: string | null;
    fileTree: FileTreeEntry[];
    buffers: EditorBuffer[];
    activeBufferId?: string;
    runningBufferId: string | null;
    renamingPath: string | null;
    formatLabel: (buffer: EditorBuffer) => string;
    onSelectBuffer: (bufferId: string) => void;
    onOpenFile: (relPath: string, options?: { preview?: boolean }) => void;
    onCreateFile: () => void;
    onCloseBuffer: (bufferId: string) => void;
    onRefreshTree: () => void;
    onRenameCommit: (path: string, newName: string) => void;
    onRenameCancel: () => void;
    onKeepBuffer: (bufferId: string) => void;
    onMoveEntry: (fromPath: string, toPath: string) => void;
}

function Section({
    title,
    action,
    actionTitle,
    onAction,
    listClassName,
    children,
}: {
    title: string;
    action: string;
    actionTitle: string;
    onAction: () => void;
    listClassName: string;
    children: ReactNode;
}) {
    return (
        <div className={`section ${listClassName}-section`}>
            <div className="section-header">
                <span>{title}</span>
                <button
                    onClick={onAction}
                    title={actionTitle}
                    className="section-action"
                >
                    {action}
                </button>
            </div>
            <div className={listClassName}>{children}</div>
        </div>
    );
}

export function FileExplorer({
    workspaceRoot,
    fileTree,
    buffers,
    activeBufferId,
    runningBufferId,
    renamingPath,
    formatLabel,
    onSelectBuffer,
    onOpenFile,
    onCreateFile,
    onCloseBuffer,
    onRefreshTree,
    onRenameCommit,
    onRenameCancel,
    onKeepBuffer,
    onMoveEntry,
}: FileExplorerProps) {
    const rootRef = useRef<HTMLDivElement>(null);
    useEffect(() => {
        const el = rootRef.current;
        if (!el) return;
        return bindFileExplorerFocus(el);
    }, []);

    const handleBufferContextMenu = (e: MouseEvent, buffer: EditorBuffer) => {
        e.preventDefault();
        void electronAPI.showContextMenu({
            type: buffer.kind,
            path: buffer.kind === 'file' ? buffer.filePath : undefined,
            bufferId: getBufferId(buffer),
            isOpenBuffer: true,
            isWorkspaceFile: false,
            x: e.clientX,
            y: e.clientY,
        });
    };

    const handleTreeContextMenu = (e: MouseEvent, entry: FileTreeEntry) => {
        e.preventDefault();

        // Buffer filePaths are absolute while tree entry paths are
        // workspace-relative.
        const absolutePath = toAbsoluteWorkspacePath(workspaceRoot, entry.path);
        const buffer = buffers.find(
            (b) => b.kind === 'file' && b.filePath === absolutePath,
        );

        void electronAPI.showContextMenu({
            bufferId: buffer ? getBufferId(buffer) : undefined,
            isOpenBuffer: !!buffer,
            isWorkspaceFile: true,
            path: entry.path,
            type: entry.type === 'directory' ? 'directory' : 'file',
            x: e.clientX,
            y: e.clientY,
        });
    };

    return (
        <div className="file-explorer" ref={rootRef}>
            <div className="file-sections">
                <Section
                    title="Open Editors"
                    action="+"
                    actionTitle="New untitled file"
                    onAction={onCreateFile}
                    listClassName="file-list"
                >
                    {buffers.length === 0 ? (
                        <div className="empty-message">No open files</div>
                    ) : (
                        <ul>
                            {buffers.map((buffer) => {
                                const bufferId = getBufferId(buffer);
                                return (
                                    <BufferItem
                                        key={bufferId}
                                        buffer={buffer}
                                        isActive={bufferId === activeBufferId}
                                        isRunning={bufferId === runningBufferId}
                                        renamingPath={renamingPath}
                                        formatLabel={formatLabel}
                                        onSelectBuffer={onSelectBuffer}
                                        onContextMenu={handleBufferContextMenu}
                                        onCloseBuffer={onCloseBuffer}
                                        onRenameCommit={onRenameCommit}
                                        onRenameCancel={onRenameCancel}
                                        onKeepBuffer={onKeepBuffer}
                                    />
                                );
                            })}
                        </ul>
                    )}
                </Section>

                {workspaceRoot && (
                    <Section
                        title="Workspace Files"
                        action="↻"
                        actionTitle="Refresh file tree"
                        onAction={onRefreshTree}
                        listClassName="file-tree"
                    >
                        {fileTree.length === 0 ? (
                            <div className="empty-message">No files found</div>
                        ) : (
                            <WorkspaceTree
                                workspaceRoot={workspaceRoot}
                                fileTree={fileTree}
                                activeFilePath={activeBufferId}
                                renamingPath={renamingPath}
                                onOpenFile={onOpenFile}
                                onContextMenu={handleTreeContextMenu}
                                onRenameCommit={onRenameCommit}
                                onRenameCancel={onRenameCancel}
                                onMove={onMoveEntry}
                            />
                        )}
                    </Section>
                )}
            </div>
        </div>
    );
}
