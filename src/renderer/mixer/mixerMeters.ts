import type { VuMeterFrame } from '@modular/core';
import type { VuControl, VuMeterDef } from '../../shared/dsl/vuMeterTypes';
import { outGainToDb } from '../../shared/dsl/vuMeterTypes';
import type { VuBallistics } from '../app/vuMeter';
import {
    drawVuMeter,
    formatDb,
    isClipping,
    newBallistics,
    updateBallistics,
    voltsToDb,
} from '../app/vuMeter';
import { readVuMeterColors, type VuMeterColors } from '../app/vuMeterColors';
import { type KnobId, paintKnob } from '../components/VuKnob';
import type { MixerStore } from './mixerStore';

interface ChannelLevels {
    rmsDb: number;
    fastDb: number;
    peakDb: number;
}

/** The bus a send knob's id names. */
export const busOfKnob = (id: KnobId): string => id.slice('send:'.length);

/** The control knob `id` sets on `output`. */
export function knobControl(output: VuMeterDef, id: KnobId): VuControl | null {
    if (id === 'pan') {
        return output.pan;
    }
    const bus = busOfKnob(id);
    return output.sends?.find((s) => s.bus === bus) ?? null;
}

/** Show a meter's highest peak on its readout pill, red above 0 dB. */
function showPeak(readout: HTMLElement, peakDb: number): void {
    readout.textContent = formatDb(peakDb);
    readout.dataset.clipped = String(isClipping(peakDb));
}

/**
 * The mixer's canvases, readouts and signal-driven knobs, painted outside
 * React: from the engine's meter frames while the clock runs, and on demand
 * for resizes and fader moves while it is stopped.
 */
export class MixerMeters {
    private readonly canvases = new Map<string, HTMLCanvasElement>();
    private readonly readouts = new Map<string, HTMLElement>();
    /** Locked knobs' SVGs by track key, then knob id. */
    private readonly knobs = new Map<string, Map<KnobId, SVGSVGElement>>();
    /** Per-channel peak-marker state by meter key. */
    private readonly ballistics = new Map<string, VuBallistics[]>();
    /** Highest peak per meter since its readout was last reset, in dB. It
     *  outlives the meter's strip, so a clip stays flagged across folds and
     *  tab switches until the readout is clicked. */
    private readonly peakMax = new Map<string, number>();
    /** Last drawn channel levels per meter, for redraws between frames. */
    private readonly lastChannels = new Map<string, ChannelLevels[]>();

    constructor(private readonly store: MixerStore) {}

    registerCanvas(key: string, canvas: HTMLCanvasElement): void {
        this.canvases.set(key, canvas);
    }

    unregisterCanvas(key: string): void {
        this.canvases.delete(key);
        this.ballistics.delete(key);
        this.lastChannels.delete(key);
    }

    registerReadout(key: string, el: HTMLElement): void {
        this.readouts.set(key, el);
        showPeak(el, this.peakMax.get(key) ?? -Infinity);
    }

    unregisterReadout(key: string): void {
        this.readouts.delete(key);
    }

    registerKnob(key: string, id: KnobId, el: SVGSVGElement): void {
        const forTrack = this.knobs.get(key) ?? new Map();
        forTrack.set(id, el);
        this.knobs.set(key, forTrack);
    }

    unregisterKnob(key: string, id: KnobId): void {
        const forTrack = this.knobs.get(key);
        forTrack?.delete(id);
        if (forTrack?.size === 0) {
            this.knobs.delete(key);
        }
    }

    /** Restart a meter's peak hold; it re-seeds from the next frame. */
    resetPeak(key: string): void {
        this.ballistics.delete(key);
        this.peakMax.delete(key);
        const readout = this.readouts.get(key);
        if (readout) {
            showPeak(readout, -Infinity);
        }
    }

    /**
     * Redraw one meter from its last drawn levels (or silence).
     * `gainDbOverride` paints an in-flight fader value.
     */
    redraw(key: string, gainDbOverride?: number | null): void {
        const canvas = this.canvases.get(key);
        if (!canvas) {
            return;
        }
        const channels =
            this.lastChannels.get(key) ??
            Array.from(
                { length: Number(canvas.dataset.channels ?? 1) },
                () => ({
                    fastDb: -Infinity,
                    peakDb: -Infinity,
                    rmsDb: -Infinity,
                }),
            );
        const output = this.store.output(key);
        const gainDb =
            gainDbOverride !== undefined
                ? gainDbOverride
                : output && output.gain.value !== null
                  ? outGainToDb(output.gain.value)
                  : null;
        this.paint(key, canvas, channels, gainDb, output, readVuMeterColors());
    }

    /** A stopped engine makes no sound: every meter falls to silence. The
     *  readouts keep their highest peaks. */
    silence(): void {
        this.ballistics.clear();
        this.lastChannels.clear();
        for (const key of this.canvases.keys()) {
            this.redraw(key);
        }
    }

    /**
     * Draw every meter from one poll of the engine. An empty poll is a lost
     * try_lock race against the audio thread's drain block, not silence, so
     * the previous frame stays on screen.
     */
    drawFrames(frames: readonly VuMeterFrame[]): void {
        if (this.canvases.size === 0 || frames.length === 0) {
            return;
        }
        const frameByModule = new Map(frames.map((f) => [f.moduleId, f]));
        const outputByKey = new Map(this.store.outputs.map((o) => [o.key, o]));
        const colors = readVuMeterColors();
        const now = performance.now();
        for (const [key, canvas] of this.canvases) {
            const frame = frameByModule.get(canvas.dataset.tapModuleId ?? '');
            if (!frame) {
                continue;
            }
            const channelCount = Number(canvas.dataset.channels ?? 1);
            let ballistics = this.ballistics.get(key);
            if (!ballistics || ballistics.length !== channelCount) {
                ballistics = Array.from(
                    { length: channelCount },
                    newBallistics,
                );
                this.ballistics.set(key, ballistics);
            }
            const channels = ballistics.map((b, ch) => {
                const rmsDb = voltsToDb(frame.rms[ch] ?? 0);
                updateBallistics(b, voltsToDb(frame.peak[ch] ?? 0), now);
                return {
                    fastDb: b.displayFastDb,
                    peakDb: b.displayPeakDb,
                    rmsDb,
                };
            });
            this.lastChannels.set(key, channels);
            const output = outputByKey.get(key);
            // A signal-driven gain shows the live value the engine sampled;
            // an editable one shows the state.
            const gainIndex = output?.gain.sourceIndex;
            const liveGain =
                gainIndex === undefined
                    ? undefined
                    : frame.controls?.[gainIndex];
            const gainValue = liveGain ?? output?.gain.value ?? null;
            this.paint(
                key,
                canvas,
                channels,
                gainValue === null ? null : outGainToDb(gainValue),
                output,
                colors,
            );
            if (frame.controls && output) {
                this.paintKnobs(output, frame.controls);
            }
            const prevMax = this.peakMax.get(key) ?? -Infinity;
            const peakMax = Math.max(prevMax, ...channels.map((c) => c.peakDb));
            if (peakMax !== prevMax) {
                this.peakMax.set(key, peakMax);
                const readout = this.readouts.get(key);
                if (readout) {
                    showPeak(readout, peakMax);
                }
            }
        }
    }

    private paint(
        key: string,
        canvas: HTMLCanvasElement,
        channels: ChannelLevels[],
        gainDb: number | null,
        output: VuMeterDef | undefined,
        colors: VuMeterColors,
    ): void {
        const ghostGain = this.store.ghost(key)?.gain;
        drawVuMeter(
            canvas,
            channels,
            colors,
            gainDb,
            output?.gain.locked === true,
            ghostGain !== undefined ? outGainToDb(ghostGain) : null,
        );
    }

    /** Paint `output`'s locked knobs from a frame's control values. */
    private paintKnobs(output: VuMeterDef, values: readonly number[]): void {
        const knobs = this.knobs.get(output.key);
        if (!knobs) {
            return;
        }
        for (const [id, svg] of knobs) {
            const index = knobControl(output, id)?.sourceIndex;
            const value = index === undefined ? undefined : values[index];
            if (value !== undefined) {
                paintKnob(svg, value);
            }
        }
    }
}
