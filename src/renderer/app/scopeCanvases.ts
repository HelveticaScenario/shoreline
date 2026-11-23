import type { ScopeStats } from '@modular/core';
import { drawOscilloscope, readScopeColors } from './oscilloscope';

/** One poll of the engine's scope buffers, by buffer key. */
export type ScopeFrames = Map<
    string,
    { data: Float32Array; stats: ScopeStats }
>;

/**
 * The editor's scope canvases, drawn outside React from each poll of the
 * engine. A canvas names its lanes and range in its data attributes.
 */
export class ScopeCanvases {
    private readonly canvases = new Map<string, HTMLCanvasElement>();

    readonly register = (key: string, canvas: HTMLCanvasElement): void => {
        this.canvases.set(key, canvas);
    };

    readonly unregister = (key: string): void => {
        this.canvases.delete(key);
    };

    draw(frames: ScopeFrames): void {
        const colors = readScopeColors();
        for (const canvas of this.canvases.values()) {
            const laneKeysStr = canvas.dataset.scopeLaneKeys;
            if (!laneKeysStr) {
                continue;
            }
            const lanes: Float32Array[] = [];
            const readOffset: number[] = [];
            const triggerOffset: number[] = [];
            let min = Infinity;
            let max = -Infinity;
            for (const laneKey of JSON.parse(laneKeysStr) as string[]) {
                const entry = frames.get(laneKey);
                if (entry) {
                    lanes.push(entry.data);
                    readOffset.push(entry.stats.readOffset);
                    triggerOffset.push(entry.stats.triggerOffset);
                    if (entry.stats.min < min) {
                        min = entry.stats.min;
                    }
                    if (entry.stats.max > max) {
                        max = entry.stats.max;
                    }
                }
            }
            if (lanes.length > 0) {
                drawOscilloscope(lanes, canvas, {
                    colors,
                    range: [
                        parseFloat(canvas.dataset.scopeRangeMin || '-5'),
                        parseFloat(canvas.dataset.scopeRangeMax || '5'),
                    ],
                    stats: {
                        max,
                        min,
                        peakToPeak: max - min,
                        readOffset,
                        triggerOffset,
                    },
                });
            }
        }
    }
}
