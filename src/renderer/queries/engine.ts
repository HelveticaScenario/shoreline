import { useQuery } from '@tanstack/react-query';
import electronAPI from '../electronAPI';

/**
 * The audio engine's CPU budget, re-read every second while a caller is
 * mounted. Dropped once the last caller unmounts, so the next one starts
 * from a fresh read.
 */
export function useEngineHealth() {
    return useQuery({
        gcTime: 0,
        queryFn: () => electronAPI.synthesizer.getHealth(),
        queryKey: ['engine', 'health'],
        refetchInterval: 1000,
    });
}

/**
 * Per-module timings, re-read every second while `enabled`. Dropped once the
 * last caller unmounts, like {@link useEngineHealth}.
 */
export function useModuleProfile(enabled: boolean) {
    return useQuery({
        enabled,
        gcTime: 0,
        queryFn: () => electronAPI.synthesizer.getModuleProfile(),
        queryKey: ['engine', 'moduleProfile'],
        refetchInterval: 1000,
    });
}

/** What the engine runs with: device, sample rate, buffer size. Re-read each
 *  time a caller becomes `enabled`. */
export function useAudioState(enabled: boolean) {
    return useQuery({
        enabled,
        queryFn: () => electronAPI.audio.getCurrentState(),
        queryKey: ['audio', 'state'],
    });
}

/** Whether the audio thread has panicked, re-read every 500 ms. A failed
 *  read keeps the last answer until the next one succeeds. */
export function useAudioPanicked() {
    return useQuery({
        queryFn: () => electronAPI.synthesizer.isAudioThreadPanicked(),
        queryKey: ['engine', 'panicked'],
        refetchInterval: 500,
    });
}

/** Where the engine writes its panic logs; it never moves. */
export function usePanicLogDir() {
    return useQuery({
        queryFn: () => electronAPI.synthesizer.panicLogDir(),
        queryKey: ['engine', 'panicLogDir'],
        staleTime: Infinity,
    });
}
