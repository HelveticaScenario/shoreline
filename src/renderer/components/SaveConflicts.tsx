import { useState } from 'react';
import type { useEditorBuffers } from '../app/hooks/useEditorBuffers';
import { getBufferId } from '../app/buffers';
import electronAPI from '../electronAPI';
import { getErrorMessage } from '../utils/errorUtils';
import { SaveConflictDiff, SaveConflictNotification } from './SaveConflict';

type Buffers = ReturnType<typeof useEditorBuffers>;

/**
 * A save that found the file changed on disk: the notification offering to
 * compare or overwrite, and the diff against the disk's content.
 */
export function SaveConflicts({
    files,
    showError,
}: {
    files: Pick<
        Buffers,
        | 'buffers'
        | 'saveConflict'
        | 'dismissSaveConflict'
        | 'overwriteOnConflict'
        | 'revertToDiskContent'
    >;
    showError: (message: string) => void;
}) {
    const { saveConflict } = files;
    const [diff, setDiff] = useState<{
        bufferId: string;
        filePath: string;
        diskContent: string;
    } | null>(null);
    const diffBuffer = diff
        ? files.buffers.find((b) => getBufferId(b) === diff.bufferId)
        : undefined;

    const compare = async () => {
        if (!saveConflict) {
            return;
        }
        try {
            const diskContent = await electronAPI.filesystem.readFile(
                saveConflict.filePath,
            );
            setDiff({ ...saveConflict, diskContent });
            files.dismissSaveConflict();
        } catch (err) {
            showError(getErrorMessage(err, 'Failed to read file'));
        }
    };

    const overwrite = async (bufferId: string) => {
        setDiff(null);
        try {
            await files.overwriteOnConflict(bufferId);
        } catch (err) {
            showError(getErrorMessage(err, 'Failed to save file'));
        }
    };

    return (
        <>
            {saveConflict && (
                <SaveConflictNotification
                    filePath={saveConflict.filePath}
                    onCompare={compare}
                    onOverwrite={() => overwrite(saveConflict.bufferId)}
                    onDismiss={files.dismissSaveConflict}
                />
            )}
            {diff && diffBuffer && (
                <SaveConflictDiff
                    filePath={diff.filePath}
                    diskContent={diff.diskContent}
                    bufferContent={diffBuffer.content}
                    onAcceptLocal={() => overwrite(diff.bufferId)}
                    onRevertLocal={() => {
                        files.revertToDiskContent(
                            diff.bufferId,
                            diff.diskContent,
                        );
                        setDiff(null);
                    }}
                    onClose={() => setDiff(null)}
                />
            )}
        </>
    );
}
