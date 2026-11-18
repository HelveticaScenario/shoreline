/** The editor cursor styles, as Monaco names them. */
export const CURSOR_STYLES = [
    'line',
    'block',
    'underline',
    'line-thin',
    'block-outline',
    'underline-thin',
] as const;

export type CursorStyle = (typeof CURSOR_STYLES)[number];

/** The cursor a config without `cursorStyle` gets. */
export const DEFAULT_CURSOR_STYLE: CursorStyle = 'line';
