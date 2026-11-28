import { type RefObject, useCallback, useState } from 'react';
import type {
    Layout,
    LayoutChangedMeta,
    PanelImperativeHandle,
} from 'react-resizable-panels';

function readStoredSize(storageKey: string): number | null {
    try {
        const stored = Number(window.localStorage.getItem(storageKey));
        if (stored > 0) return stored;
    } catch {}
    return null;
}

function writeStoredSize(storageKey: string, size: number): void {
    try {
        window.localStorage.setItem(storageKey, String(size));
    } catch {}
}

interface StoredPanelSize {
    /** The panel's `defaultSize`: the stored size at mount, else the fallback. */
    defaultSize: number;
    /** Goes on the panel's group, and saves only the resizes the user makes. */
    onLayoutChanged: (layout: Layout, meta: LayoutChangedMeta) => void;
    /** Resizes the panel to its stored size, if any. Stable across renders. */
    restore: () => void;
}

/**
 * A panel's size in pixels, remembered in localStorage across launches. A
 * size in pixels holds when the window or the panel's neighbours change.
 */
export function useStoredPanelSize(
    storageKey: string,
    fallback: number,
    panelRef: RefObject<PanelImperativeHandle | null>,
): StoredPanelSize {
    const [defaultSize] = useState(
        () => readStoredSize(storageKey) ?? fallback,
    );
    const restore = useCallback(() => {
        const stored = readStoredSize(storageKey);
        if (stored !== null) panelRef.current?.resize(stored);
    }, [storageKey, panelRef]);

    // The new layout reaches the DOM after this callback, so the size is
    // read on the next frame.
    const onLayoutChanged = (
        _: Layout,
        { isUserInteraction }: LayoutChangedMeta,
    ) => {
        if (!isUserInteraction) return;
        requestAnimationFrame(() => {
            const panel = panelRef.current;
            if (!panel || panel.isCollapsed()) return;
            writeStoredSize(storageKey, Math.round(panel.getSize().inPixels));
        });
    };

    return { defaultSize, onLayoutChanged, restore };
}
