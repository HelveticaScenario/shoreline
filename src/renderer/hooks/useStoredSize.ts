import { useEffect, useState } from 'react';

/**
 * A panel size in pixels, remembered in localStorage across launches. `null`
 * means the stylesheet's default size; `clamp` fits a stored size to the
 * current layout when it is read back.
 */
export function useStoredSize(
    storageKey: string,
    clamp: (size: number) => number,
): [number | null, (size: number | null) => void] {
    const [size, setSize] = useState<number | null>(() => {
        try {
            const stored = Number(window.localStorage.getItem(storageKey));
            if (stored > 0) return clamp(stored);
        } catch {}
        return null;
    });

    useEffect(() => {
        try {
            if (size === null) window.localStorage.removeItem(storageKey);
            else window.localStorage.setItem(storageKey, String(size));
        } catch {}
    }, [storageKey, size]);

    return [size, setSize];
}
