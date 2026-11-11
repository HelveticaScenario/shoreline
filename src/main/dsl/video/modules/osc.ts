import type { VideoModuleDef } from './types';

/** Each shape's level at phase `p`, a fraction of a cycle in 0..1. */
const SHAPES: Record<string, string> = {
    sine: '0.5 + 0.5 * sin(6.28318530718 * p)',
    triangle: 'abs(2.0 * p - 1.0)',
    saw: 'p',
    square: 'step(0.5, p)',
};

/**
 * Periodic shaper: `freq` cycles per unit of `input`, offset by `phase`
 * cycles. Red runs `offset` cycles behind and blue `offset` cycles ahead of
 * green; a constant offset of 0 compiles to a single gray wave.
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
        `fn osc_${shape}(p: f32) -> f32 {
    return ${SHAPES[shape]};
}`,
        `fn osc_${shape}_split(x: f32, spread: f32) -> vec4f {
    return vec4f(osc_${shape}(fract(x - spread)), osc_${shape}(fract(x)), osc_${shape}(fract(x + spread)), 1.0);
}`,
    ],
    emit: ({ input, freq, phase, offset }, { shape }) => {
        const x = `${input} * ${freq} + ${phase}`;
        return offset === '0.0'
            ? `vec4f(vec3f(osc_${shape}(fract(${x}))), 1.0)`
            : `osc_${shape}_split(${x}, ${offset})`;
    },
};
