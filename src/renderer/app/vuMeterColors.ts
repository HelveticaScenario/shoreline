/**
 * The colours a VU meter draws in, read from the theme's CSS variables. A
 * clipping meter's reds derive from the theme's error colour.
 */

interface ClipShades {
    /** The theme's error colour these shades derive from. */
    hot: string;
    /** Below −12 dB. */
    low: string;
    /** −12 to 0 dB. */
    mid: string;
    /** The peak-hold dash. */
    peak: string;
}

const FALLBACK_HOT = '#e05561';

let parseCtx: CanvasRenderingContext2D | null = null;

/** Any CSS colour as [r, g, b], read back from a pixel painted in it so every
 *  syntax the canvas accepts resolves. An invalid colour reads as the
 *  fallback red. */
function parseRgb(color: string): [number, number, number] {
    parseCtx ??= document
        .createElement('canvas')
        .getContext('2d', { willReadFrequently: true });
    if (!parseCtx) {
        return [0xe0, 0x55, 0x61];
    }
    parseCtx.clearRect(0, 0, 1, 1);
    parseCtx.fillStyle = FALLBACK_HOT;
    parseCtx.fillStyle = color;
    parseCtx.fillRect(0, 0, 1, 1);
    const [r, g, b] = parseCtx.getImageData(0, 0, 1, 1).data;
    return [r, g, b];
}

/** `color` moved `amount` (0–1) of the way to `target`. */
function mixRgb(
    color: [number, number, number],
    target: [number, number, number],
    amount: number,
): string {
    const channel = (i: number) =>
        Math.round(color[i] + (target[i] - color[i]) * amount);
    return `rgb(${channel(0)}, ${channel(1)}, ${channel(2)})`;
}

let cached: ClipShades | null = null;

/** The clip reds for the theme's error colour, recomputed only when the
 *  theme changes it. */
function clipShadesFor(hot: string): ClipShades {
    if (cached?.hot !== hot) {
        const rgb = parseRgb(hot);
        cached = {
            hot,
            low: mixRgb(rgb, [0, 0, 0], 0.45),
            mid: mixRgb(rgb, [0, 0, 0], 0.2),
            peak: mixRgb(rgb, [255, 255, 255], 0.55),
        };
    }
    return cached;
}

export interface VuMeterColors {
    bg: string;
    track: string;
    border: string;
    muted: string;
    low: string;
    mid: string;
    hot: string;
    /** Externally-controlled (signal-driven, locked) control accents. */
    external: string;
    /** A clipping bar's lower zones and peak dash, shades of `hot`, which
     *  fills the zone above 0 dB in both modes. */
    clipLow: string;
    clipMid: string;
    clipPeak: string;
    /** Font family for the dB scale labels (the `--font-mono` CSS var). */
    fontMono: string;
}

export function readVuMeterColors(): VuMeterColors {
    const styles = getComputedStyle(document.documentElement);
    const hot = styles.getPropertyValue('--color-error').trim() || '#e05561';
    const clip = clipShadesFor(hot);
    return {
        bg: styles.getPropertyValue('--bg-primary').trim() || '#0a0a0a',
        track: '#141414',
        border: styles.getPropertyValue('--border-subtle').trim() || '#222222',
        muted: styles.getPropertyValue('--text-muted').trim() || '#8a8a8a',
        low: styles.getPropertyValue('--color-success').trim() || '#9fd94f',
        mid: styles.getPropertyValue('--color-warning').trim() || '#d7ba4a',
        hot,
        external:
            styles.getPropertyValue('--accent-primary').trim() || '#4ec9b0',
        clipLow: clip.low,
        clipMid: clip.mid,
        clipPeak: clip.peak,
        fontMono: styles.getPropertyValue('--font-mono').trim() || 'monospace',
    };
}
