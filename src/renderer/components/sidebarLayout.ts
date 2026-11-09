import { type CSSProperties, useEffect, useState } from 'react';

export type SidebarSide = 'left' | 'right';

const WIDTH_STORAGE_KEY = 'modular_sidebar_width';
const SIDE_STORAGE_KEY = 'modular_sidebar_side';
const OPEN_STORAGE_KEY = 'modular_sidebar_open';
export const SIDEBAR_MIN_WIDTH = 160;
/** Room kept for the editor and the activity bar when the sidebar grows. */
const RESERVED_WIDTH = 280;

export function sidebarMaxWidth(): number {
    return Math.max(SIDEBAR_MIN_WIDTH, window.innerWidth - RESERVED_WIDTH);
}

function clampWidth(width: number): number {
    return Math.round(
        Math.min(Math.max(width, SIDEBAR_MIN_WIDTH), sidebarMaxWidth()),
    );
}

/**
 * The sidebar panel's sizing. A narrowed window squeezes the sidebar before
 * the editor; with no width set, the stylesheet's default applies.
 */
export function sidebarPanelStyle(width: number | null): CSSProperties {
    return {
        width: width ?? undefined,
        minWidth: SIDEBAR_MIN_WIDTH,
        maxWidth: `max(${SIDEBAR_MIN_WIDTH}px, calc(100vw - ${RESERVED_WIDTH}px))`,
    };
}

/**
 * The sidebar panel's width in pixels, remembered across launches. `null`
 * means the stylesheet's default width.
 */
export function useSidebarWidth(): [
    number | null,
    (width: number | null) => void,
] {
    const [width, setWidth] = useState<number | null>(() => {
        try {
            const stored = Number(
                window.localStorage.getItem(WIDTH_STORAGE_KEY),
            );
            if (stored > 0) return clampWidth(stored);
        } catch {}
        return null;
    });

    useEffect(() => {
        try {
            if (width === null)
                window.localStorage.removeItem(WIDTH_STORAGE_KEY);
            else window.localStorage.setItem(WIDTH_STORAGE_KEY, String(width));
        } catch {}
    }, [width]);

    return [width, setWidth];
}

/** Whether the sidebar panel is open, remembered across launches. */
export function useSidebarOpen(): [boolean, (open: boolean) => void] {
    const [open, setOpen] = useState(() => {
        try {
            return window.localStorage.getItem(OPEN_STORAGE_KEY) !== 'false';
        } catch {}
        return true;
    });

    useEffect(() => {
        try {
            window.localStorage.setItem(OPEN_STORAGE_KEY, String(open));
        } catch {}
    }, [open]);

    return [open, setOpen];
}

/**
 * The configured side, remembered here as well so the sidebar opens on that
 * side before the config file has been read.
 */
export function useSidebarSide(
    configured: SidebarSide | undefined,
    loaded: boolean,
): SidebarSide {
    const [remembered] = useState<SidebarSide>(() => {
        try {
            if (window.localStorage.getItem(SIDE_STORAGE_KEY) === 'left')
                return 'left';
        } catch {}
        return 'right';
    });
    const side = loaded ? (configured ?? 'right') : remembered;

    useEffect(() => {
        if (!loaded) return;
        try {
            window.localStorage.setItem(SIDE_STORAGE_KEY, side);
        } catch {}
    }, [loaded, side]);

    return side;
}
