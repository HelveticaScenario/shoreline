import type { VideoModuleDef } from './types';

/** Each shape's level at phase `p`, a fraction of a cycle in 0..1. */
const SHAPES: Record<string, string> = {
    sine: '0.5 + 0.5 * sin(6.28318530718 * p)',
    triangle: 'abs(2.0 * p - 1.0)',
    saw: 'p',
    square: 'step(vec3f(0.5), p)',
};

/**
 * Periodic shaper: `freq` cycles per unit of `input`, offset by `phase`
 * cycles. Red runs `offset` cycles behind and blue `offset` cycles ahead of
 * green, so an offset of 0 is gray.
 */
export const osc: VideoModuleDef = {
    inputs: {
        input: 'number',
        freq: 'number',
        phase: 'number',
        offset: 'number',
    },
    output: 'rgba',
    params: {
        shape: { values: Object.keys(SHAPES), default: 'sine' },
    },
    helpers: ({ shape }) => [
        `fn osc_${shape}(x: f32, spread: f32) -> vec4f {
    let p = fract(vec3f(x - spread, x, x + spread));
    return vec4f(${SHAPES[shape]}, 1.0);
}`,
    ],
    emit: ({ input, freq, phase, offset }, { shape }) =>
        `osc_${shape}(${input} * ${freq} + ${phase}, ${offset})`,
};
