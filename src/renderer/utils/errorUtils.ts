import type { FSWriteResult } from '../../shared/ipcTypes';

export const getErrorMessage = (error: unknown, fallback: string): string =>
    error instanceof Error ? error.message : fallback;

/** The error a failed filesystem operation is reported with. */
export const fsError = (result: FSWriteResult, fallback: string): Error =>
    new Error(('error' in result && result.error) || fallback);
