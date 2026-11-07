import {
    type CSSProperties,
    type KeyboardEvent,
    type PointerEvent,
    useEffect,
    useRef,
    useState,
} from 'react';

const WIDTH_STORAGE_KEY = 'modular_sidebar_width';
const MIN_WIDTH = 160;
/** Room kept for the editor and the activity bar when the sidebar grows. */
const RESERVED_WIDTH = 280;
const KEYBOARD_STEP = 16;

function maxWidth(): number {
    return Math.max(MIN_WIDTH, window.innerWidth - RESERVED_WIDTH);
}

function clampWidth(width: number): number {
    return Math.round(Math.min(Math.max(width, MIN_WIDTH), maxWidth()));
}

/**
 * The sidebar panel's sizing. A narrowed window squeezes the sidebar before
 * the editor; with no width set, the stylesheet's default applies.
 */
export function sidebarPanelStyle(width: number | null): CSSProperties {
    return {
        width: width ?? undefined,
        minWidth: MIN_WIDTH,
        maxWidth: `max(${MIN_WIDTH}px, calc(100vw - ${RESERVED_WIDTH}px))`,
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

interface SidebarSashProps {
    panelId: string;
    onWidthChange: (width: number | null) => void;
}

/**
 * The drag handle on the sidebar's editor-facing edge. Dragging toward the
 * editor widens the panel; double-clicking restores the default width.
 */
export function SidebarSash({ panelId, onWidthChange }: SidebarSashProps) {
    const drag = useRef<{ startX: number; startWidth: number } | null>(null);
    const [dragging, setDragging] = useState(false);
    const [rendered, setRendered] = useState<{ now: number; max: number }>();

    // The drawn width, which the window's size may hold below the stored one.
    const renderedWidth = () =>
        document.getElementById(panelId)?.getBoundingClientRect().width ??
        MIN_WIDTH;

    useEffect(() => {
        const panel = document.getElementById(panelId);
        if (!panel) return;
        const measure = () =>
            setRendered({
                now: Math.round(panel.getBoundingClientRect().width),
                max: maxWidth(),
            });
        const observer = new ResizeObserver(measure);
        observer.observe(panel);
        window.addEventListener('resize', measure);
        return () => {
            observer.disconnect();
            window.removeEventListener('resize', measure);
        };
    }, [panelId]);

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
        drag.current = { startX: e.clientX, startWidth: renderedWidth() };
        setDragging(true);
    };

    const onPointerMove = (e: PointerEvent<HTMLDivElement>) => {
        if (!drag.current) return;
        const { startX, startWidth } = drag.current;
        if (e.clientX === startX) return;
        onWidthChange(clampWidth(startWidth + startX - e.clientX));
    };

    // Fires after pointerup and pointercancel too, and when the window takes
    // capture away without either.
    const endDrag = () => {
        drag.current = null;
        setDragging(false);
    };

    const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
        const step = e.shiftKey ? KEYBOARD_STEP * 4 : KEYBOARD_STEP;
        if (e.key === 'ArrowLeft') {
            onWidthChange(clampWidth(renderedWidth() + step));
        } else if (e.key === 'ArrowRight') {
            onWidthChange(clampWidth(renderedWidth() - step));
        } else if (e.key === 'Home') {
            onWidthChange(MIN_WIDTH);
        } else if (e.key === 'End') {
            onWidthChange(maxWidth());
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
            aria-valuemax={rendered?.max}
            aria-valuenow={rendered?.now}
            tabIndex={0}
            className={`app-sidebar-sash ${dragging ? 'active' : ''}`}
            onPointerDown={onPointerDown}
            onPointerMove={onPointerMove}
            onLostPointerCapture={endDrag}
            onDoubleClick={() => onWidthChange(null)}
            onKeyDown={onKeyDown}
        />
    );
}
