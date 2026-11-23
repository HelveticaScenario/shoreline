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
 * Polls the engine once per animation frame while the clock runs and hands
 * each poll to `onFrame`. While the clock is stopped and Link is enabled it
 * polls the transport alone, so the Link phase indicator stays live before
 * play.
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
        let cancelled = false;
        const tick = () => {
            if (cancelled) {
                return;
            }
            void poll()
                .then(([scopeData, transport, meters]) => {
                    if (cancelled) {
                        return;
                    }
                    const scopes: ScopeFrames = new Map();
                    for (const [bufferKey, data, stats] of scopeData) {
                        scopes.set(scopeBufferKeyToString(bufferKey), {
                            data,
                            stats,
                        });
                    }
                    deliver({ meters, scopes, transport });
                })
                .catch((err) => {
                    console.error('Failed to get scopes:', err);
                })
                .then(() => {
                    if (!cancelled) {
                        requestAnimationFrame(tick);
                    }
                });
        };
        requestAnimationFrame(tick);
        return () => {
            cancelled = true;
        };
    }, [running]);

    useEffect(() => {
        if (!linkEnabled || running) {
            return;
        }
        let cancelled = false;
        let rafId = 0;
        const tick = () => {
            if (cancelled) {
                return;
            }
            void electronAPI.synthesizer.getTransportState().then((t) => {
                if (cancelled) {
                    return;
                }
                setTransport(t);
                rafId = requestAnimationFrame(tick);
            });
        };
        rafId = requestAnimationFrame(tick);
        return () => {
            cancelled = true;
            cancelAnimationFrame(rafId);
        };
    }, [linkEnabled, running]);
}
