import { queryOptions, useQuery } from '@tanstack/react-query';
import type { AppConfig } from '../../shared/ipcTypes';
import electronAPI from '../electronAPI';
import { queryClient } from './client';

/** The config file. It is never stale: `watchConfig` replaces it on every
 *  change to the file, from this window or outside it. */
export const configQuery = queryOptions({
    queryFn: () => electronAPI.config.read(),
    queryKey: ['config'],
    staleTime: Infinity,
});

const EMPTY_CONFIG: AppConfig = {};

/** Keeps the cached config equal to the file for the life of the window. */
export function watchConfig(): void {
    electronAPI.config.onChange((config) => {
        queryClient.setQueryData(configQuery.queryKey, config);
    });
}

/**
 * The config file, empty until it has been read, and whether it has been
 * read.
 */
export function useConfig(): { config: AppConfig; loaded: boolean } {
    const { data, isSuccess } = useQuery(configQuery);
    return { config: data ?? EMPTY_CONFIG, loaded: isSuccess };
}

/**
 * Merges `partial` into the config and saves it. Only the changed fields are
 * written, so a copy that has not yet caught up with another edit never
 * writes that edit back over.
 */
export function changeConfig(partial: Partial<AppConfig>): void {
    queryClient.setQueryData(configQuery.queryKey, (prev) => ({
        ...prev,
        ...partial,
    }));
    electronAPI.config.write(partial).catch(console.error);
}
