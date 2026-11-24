import { useEffect, type Dispatch } from 'react';
import electronAPI from '../../electronAPI';
import type { EditorBuffer } from '../../types/editor';
import type { BufferAction } from '../bufferState';

/**
 * Keeps file buffers in step with their files: open files are watched for
 * external changes, and buffers restored from storage are compared against
 * their files as they are on startup.
 */
export function useBufferDiskSync(
    buffers: EditorBuffer[],
    restoredBuffers: EditorBuffer[],
    dispatch: Dispatch<BufferAction>,
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
            electronAPI.filesystem.onFileChanged(({ filePath, content }) =>
                dispatch({ content, filePath, type: 'diskChanged' }),
            ),
        [dispatch],
    );

    useEffect(() => {
        for (const buffer of restoredBuffers) {
            if (buffer.kind !== 'file') {
                continue;
            }
            const { filePath, savedContent } = buffer;
            electronAPI.filesystem
                .readFile(filePath)
                .then((diskContent) =>
                    dispatch({
                        baseline: savedContent,
                        diskContent,
                        filePath,
                        type: 'restoredFileRead',
                    }),
                )
                .catch(() => {
                    // A missing or unreadable file keeps the buffer dirty.
                });
        }
    }, [restoredBuffers, dispatch]);
}
