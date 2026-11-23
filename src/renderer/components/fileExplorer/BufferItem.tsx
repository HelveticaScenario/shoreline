import type { MouseEvent } from 'react';
import type { EditorBuffer } from '../../types/editor';
import { getBufferId } from '../../app/buffers';
import { RenameInput } from './RenameInput';

interface BufferItemProps {
    buffer: EditorBuffer;
    isActive: boolean;
    isRunning: boolean;
    renamingPath: string | null;
    formatLabel: (buffer: EditorBuffer) => string;
    onSelectBuffer: (id: string) => void;
    onContextMenu: (e: MouseEvent, buffer: EditorBuffer) => void;
    onCloseBuffer: (id: string) => void;
    onRenameCommit: (path: string, newName: string) => void;
    onRenameCancel: () => void;
    onKeepBuffer: (id: string) => void;
}

/** A row of the Open Editors list. */
export function BufferItem({
    buffer,
    isActive,
    isRunning,
    renamingPath,
    formatLabel,
    onSelectBuffer,
    onContextMenu,
    onCloseBuffer,
    onRenameCommit,
    onRenameCancel,
    onKeepBuffer,
}: BufferItemProps) {
    const bufferId = getBufferId(buffer);
    const renamingFilePath =
        buffer.kind === 'file' && renamingPath === buffer.filePath
            ? buffer.filePath
            : null;
    const isRenaming = renamingFilePath !== null;
    const label = formatLabel(buffer);

    return (
        <li
            className={[
                'buffer-item',
                isActive ? 'active' : '',
                buffer.dirty ? 'dirty' : '',
                isRunning ? 'running' : '',
                buffer.isPreview ? 'preview' : '',
            ]
                .filter(Boolean)
                .join(' ')}
            onClick={() => !isRenaming && onSelectBuffer(bufferId)}
            onDoubleClick={() => !isRenaming && onKeepBuffer(bufferId)}
            onContextMenu={(e) => onContextMenu(e, buffer)}
        >
            {renamingFilePath !== null ? (
                <RenameInput
                    name={label}
                    onCommit={(newName) =>
                        onRenameCommit(renamingFilePath, newName)
                    }
                    onCancel={onRenameCancel}
                    onClick={(e) => e.stopPropagation()}
                />
            ) : (
                <span className="file-name">{label}</span>
            )}
            {!isRenaming && isRunning && (
                <span className="running-badge">▶</span>
            )}
            {!isRenaming && buffer.dirty && (
                <span className="dirty-dot">●</span>
            )}
            {!isRenaming && (
                <button
                    className="close-button"
                    onClick={(e) => {
                        e.stopPropagation();
                        onCloseBuffer(bufferId);
                    }}
                    title="Close"
                >
                    ×
                </button>
            )}
        </li>
    );
}
