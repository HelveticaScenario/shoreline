import { useEffect, type Dispatch, type SetStateAction } from 'react';
import electronAPI from '../../electronAPI';
import type { EditorBuffer } from '../../types/editor';
import { updateFileBuffer } from '../bufferList';
import { withDiskChange, withSavedContent } from '../buffers';

/**
 * Keeps file buffers in step with their files: open files are watched for
 * external changes, and buffers restored from storage are compared against
 * their files as they are on startup.
 */
export function useBufferDiskSync(
    buffers: EditorBuffer[],
    restoredBuffers: EditorBuffer[],
    setBuffers: Dispatch<SetStateAction<EditorBuffer[]>>,
) {
    const watchedPathsKey = JSON.stringify(
        buffers.flatMap((b) => (b.kind === 'file' ? [b.filePath] : [])).sort(),
    );
    useEffect(() => {
        electronAPI.filesystem
            .watchOpenFiles(JSON.parse(watchedPathsKey) as string[])
            .catch((error: unknown) =>
                console.error('Failed to watch open files:', error),
            );
    }, [watchedPathsKey]);

    useEffect(
        () =>
            electronAPI.filesystem.onFileChanged(({ filePath, content }) => {
                setBuffers((prev) =>
                    updateFileBuffer(prev, filePath, (b) =>
                        withDiskChange(b, content),
                    ),
                );
            }),
        [setBuffers],
    );

    // A restored buffer whose baseline is unknown adopts the disk content as
    // its baseline; one with a baseline treats the disk content like an
    // external change, so edits made against an older file still conflict on
    // save.
    useEffect(() => {
        for (const buffer of restoredBuffers) {
            if (buffer.kind !== 'file') {
                continue;
            }
            const { filePath, savedContent } = buffer;
            electronAPI.filesystem
                .readFile(filePath)
                .then((diskContent) => {
                    setBuffers((prev) =>
                        updateFileBuffer(prev, filePath, (b) => {
                            // A save or reload since the read makes it stale.
                            if (b.savedContent !== savedContent) {
                                return b;
                            }
                            return b.savedContent === null
                                ? withSavedContent(b, diskContent)
                                : withDiskChange(b, diskContent);
                        }),
                    );
                })
                .catch(() => {
                    // A missing or unreadable file keeps the buffer dirty.
                });
        }
    }, [restoredBuffers, setBuffers]);
}
