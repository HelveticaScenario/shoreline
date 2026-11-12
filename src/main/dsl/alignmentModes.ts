/**
 * The ways `.trans` lines its two patterns up, after Strudel. They mirror the
 * Rust `SpAlignmentMode` enum; bare `.trans(...)` is `in`.
 */
export const ALIGNMENT_MODES = [
    'in',
    'out',
    'mix',
    'squeeze',
    'squeezeout',
    'reset',
    'restart',
] as const;
export type AlignmentMode = (typeof ALIGNMENT_MODES)[number];
