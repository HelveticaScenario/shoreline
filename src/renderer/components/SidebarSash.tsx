import {
    type KeyboardEvent,
    type PointerEvent,
    useCallback,
    useEffect,
    useRef,
    useState,
} from 'react';

const WIDTH_STORAGE_KEY = 'modular_sidebar_width';
const MIN_WIDTH = 160;
/** Room kept for the editor and the activity bar when the sidebar grows. */
const RESERVED_WIDTH = 280;
const KEYBOARD_STEP = 16;

function clampWidth(width: number): number {
    const max = Math.max(MIN_WIDTH, window.innerWidth - RESERVED_WIDTH);
    return Math.round(Math.min(Math.max(width, MIN_WIDTH), max));
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
            if (stored > 0) return stored;
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

interface SidebarSashProps {
    panelId: string;
    width: number | null;
    onWidthChange: (width: number | null) => void;
}

/**
 * The drag handle on the sidebar's editor-facing edge. Dragging toward the
 * editor widens the panel; double-clicking restores the default width.
 */
export function SidebarSash({
    panelId,
    width,
    onWidthChange,
}: SidebarSashProps) {
    const drag = useRef<{ startX: number; startWidth: number } | null>(null);
    const [dragging, setDragging] = useState(false);

    const currentWidth = useCallback(
        () =>
            width ??
            document.getElementById(panelId)?.getBoundingClientRect().width ??
            MIN_WIDTH,
        [panelId, width],
    );

    // The cursor and selection lock live on the body so they hold while the
    // pointer is over the editor during the drag.
    useEffect(() => {
        if (!dragging) return;
        document.body.classList.add('app-sidebar-resizing');
        return () => document.body.classList.remove('app-sidebar-resizing');
    }, [dragging]);

    const onPointerDown = (e: PointerEvent<HTMLDivElement>) => {
        if (e.button !== 0) return;
        e.preventDefault();
        e.currentTarget.setPointerCapture(e.pointerId);
        drag.current = { startX: e.clientX, startWidth: currentWidth() };
        setDragging(true);
    };

    const onPointerMove = (e: PointerEvent<HTMLDivElement>) => {
        if (!drag.current) return;
        const { startX, startWidth } = drag.current;
        onWidthChange(clampWidth(startWidth + startX - e.clientX));
    };

    const endDrag = () => {
        drag.current = null;
        setDragging(false);
    };

    const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
        const step = e.shiftKey ? KEYBOARD_STEP * 4 : KEYBOARD_STEP;
        if (e.key === 'ArrowLeft') {
            onWidthChange(clampWidth(currentWidth() + step));
        } else if (e.key === 'ArrowRight') {
            onWidthChange(clampWidth(currentWidth() - step));
        } else if (e.key === 'Home') {
            onWidthChange(MIN_WIDTH);
        } else if (e.key === 'Enter') {
            onWidthChange(null);
        } else {
            return;
        }
        e.preventDefault();
    };

    return (
        <div
            role="separator"
            aria-orientation="vertical"
            aria-controls={panelId}
            aria-label="Resize sidebar"
            aria-valuemin={MIN_WIDTH}
            aria-valuenow={width ?? undefined}
            tabIndex={0}
            className={`app-sidebar-sash ${dragging ? 'active' : ''}`}
            onPointerDown={onPointerDown}
            onPointerMove={onPointerMove}
            onPointerUp={endDrag}
            onPointerCancel={endDrag}
            onDoubleClick={() => onWidthChange(null)}
            onKeyDown={onKeyDown}
        />
    );
}
