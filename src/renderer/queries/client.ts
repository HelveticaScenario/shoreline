import { QueryClient } from '@tanstack/react-query';

/**
 * The renderer's one cache of main-process state. Main pushes changes to the
 * data that changes outside the renderer (see `watchConfig`), so a window
 * regaining focus refetches nothing, and a failed IPC call surfaces at once.
 */
export const queryClient = new QueryClient({
    defaultOptions: {
        queries: {
            refetchOnWindowFocus: false,
            retry: false,
        },
    },
});
