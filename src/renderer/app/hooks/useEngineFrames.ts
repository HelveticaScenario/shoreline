import { useEffect, useEffectEvent } from 'react';
import type { VuMeterFrame } from '@modular/core';
import type { TransportSnapshot } from '../../../shared/ipcTypes';
import electronAPI from '../../electronAPI';
import { scopeBufferKeyToString } from '../oscilloscope';
import type { ScopeFrames } from '../scopeCanvases';
import { setTransport, useTransportLinkEnabled } from '../transportStore';

export interface EngineFrame {
    scopes: ScopeFrames;
    /** Empty while `pollMeters` is off. */
    meters: VuMeterFrame[];
    transport: TransportSnapshot;
}

/**
 * Runs `step` once per animation frame, each frame waiting for the one
 * before to settle. `step` checks `isLive` after it awaits, before acting on
 * what it read. Returns the function that stops the loop.
 */
function startFrameLoop(
    step: (isLive: () => boolean) => Promise<void>,
): () => void {
    let cancelled = false;
    let rafId = 0;
    const isLive = () => !cancelled;
    const tick = () => {
        void step(isLive)
            .catch((err) => {
                console.error('Failed to poll the engine:', err);
            })
            .then(() => {
                if (!cancelled) {
                    rafId = requestAnimationFrame(tick);
                }
            });
    };
    rafId = requestAnimationFrame(tick);
    return () => {
        cancelled = true;
        cancelAnimationFrame(rafId);
    };
}

/**
 * Polls the engine once per animation frame while the clock runs: each poll
 * updates the transport store, then goes to `onFrame`. While the clock is
 * stopped and Link is enabled it polls the transport alone, so the Link phase
 * indicator stays live before play.
 */
export function useEngineFrames({
    running,
    pollMeters,
    onFrame,
}: {
    running: boolean;
    /** Whether to poll the mixer's meters; a hidden mixer costs no IPC. */
    pollMeters: boolean;
    onFrame: (frame: EngineFrame) => void;
}): void {
    const linkEnabled = useTransportLinkEnabled();

    const poll = useEffectEvent(() =>
        Promise.all([
            electronAPI.synthesizer.getScopes(),
            electronAPI.synthesizer.getTransportState(),
            pollMeters
                ? electronAPI.synthesizer.getVuMeters()
                : Promise.resolve([]),
        ]),
    );
    const deliver = useEffectEvent(onFrame);

    useEffect(() => {
        if (!running) {
            return;
        }
        return startFrameLoop(async (isLive) => {
            const [scopeData, transport, meters] = await poll();
            if (!isLive()) {
                return;
            }
            const scopes: ScopeFrames = new Map();
            for (const [bufferKey, data, stats] of scopeData) {
                scopes.set(scopeBufferKeyToString(bufferKey), { data, stats });
            }
            setTransport(transport);
            deliver({ meters, scopes, transport });
        });
    }, [running]);

    useEffect(() => {
        if (!linkEnabled || running) {
            return;
        }
        return startFrameLoop(async (isLive) => {
            const transport = await electronAPI.synthesizer.getTransportState();
            if (isLive()) {
                setTransport(transport);
            }
        });
    }, [linkEnabled, running]);
}
