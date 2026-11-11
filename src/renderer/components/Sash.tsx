import {
    type KeyboardEvent,
    type PointerEvent,
    useEffect,
    useRef,
    useState,
} from 'react';
import './Sash.css';

const KEYBOARD_STEP = 16;

interface SashProps {
    /** The id of the panel this sash sizes, measured for its drawn size. */
    panelId: string;
    /** `x` sizes the panel's width, `y` its height. */
    axis: 'x' | 'y';
    /** The pointer direction along the axis that grows the panel. */
    grow: 1 | -1;
    min: number;
    /** The largest size the layout allows, read at each use; keep it stable. */
    max: () => number;
    label: string;
    className: string;
    /** `null` restores the default size. */
    onSizeChange: (size: number | null) => void;
}

/**
 * The drag handle on a panel's editor-facing edge. Dragging toward the editor
 * grows the panel; double-clicking restores the default size.
 */
export function Sash({
    panelId,
    axis,
    grow,
    min,
    max,
    label,
    className,
    onSizeChange,
}: SashProps) {
    const drag = useRef<{ start: number; startSize: number } | null>(null);
    const [dragging, setDragging] = useState(false);
    const [rendered, setRendered] = useState<{ now: number; max: number }>();

    const clamp = (size: number) =>
        Math.round(Math.min(Math.max(size, min), Math.max(min, max())));

    // The drawn size, which the window's size may hold below the stored one.
    const renderedSize = () => {
        const rect = document.getElementById(panelId)?.getBoundingClientRect();
        if (!rect) return min;
        return axis === 'x' ? rect.width : rect.height;
    };

    useEffect(() => {
        const panel = document.getElementById(panelId);
        if (!panel) return;
        const measure = () => {
            const rect = panel.getBoundingClientRect();
            setRendered({
                now: Math.round(axis === 'x' ? rect.width : rect.height),
                max: Math.max(min, max()),
            });
        };
        const observer = new ResizeObserver(measure);
        observer.observe(panel);
        // The largest size can follow the container as well as the window.
        if (panel.parentElement) observer.observe(panel.parentElement);
        window.addEventListener('resize', measure);
        return () => {
            observer.disconnect();
            window.removeEventListener('resize', measure);
        };
    }, [panelId, axis, min, max]);

    // The cursor and selection lock live on the body so they hold while the
    // pointer is over the editor during the drag.
    useEffect(() => {
        if (!dragging) return;
        const bodyClass = `app-sash-resizing-${axis}`;
        document.body.classList.add(bodyClass);
        return () => document.body.classList.remove(bodyClass);
    }, [dragging, axis]);

    // A sash with no room to move leaves the stored size alone.
    const resize = (size: number) => {
        if (max() > min) onSizeChange(size);
    };

    const coordinate = (e: PointerEvent) =>
        axis === 'x' ? e.clientX : e.clientY;

    const onPointerDown = (e: PointerEvent<HTMLDivElement>) => {
        if (e.button !== 0) return;
        e.preventDefault();
        e.currentTarget.setPointerCapture(e.pointerId);
        drag.current = { start: coordinate(e), startSize: renderedSize() };
        setDragging(true);
    };

    const onPointerMove = (e: PointerEvent<HTMLDivElement>) => {
        if (!drag.current) return;
        const { start, startSize } = drag.current;
        const position = coordinate(e);
        if (position === start) return;
        resize(clamp(startSize + (position - start) * grow));
    };

    // Fires after pointerup and pointercancel too, and when the window takes
    // capture away without either.
    const endDrag = () => {
        drag.current = null;
        setDragging(false);
    };

    const [decrease, increase] =
        axis === 'x' ? ['ArrowLeft', 'ArrowRight'] : ['ArrowUp', 'ArrowDown'];

    const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
        const step = e.shiftKey ? KEYBOARD_STEP * 4 : KEYBOARD_STEP;
        if (e.key === decrease || e.key === increase) {
            const direction = e.key === decrease ? -1 : 1;
            resize(clamp(renderedSize() + step * direction * grow));
        } else if (e.key === 'Home') {
            resize(min);
        } else if (e.key === 'End') {
            resize(clamp(Infinity));
        } else {
            return;
        }
        e.preventDefault();
    };

    return (
        <div
            role="separator"
            aria-orientation={axis === 'x' ? 'vertical' : 'horizontal'}
            aria-controls={panelId}
            aria-label={label}
            aria-valuemin={min}
            aria-valuemax={rendered?.max}
            aria-valuenow={rendered?.now}
            tabIndex={0}
            className={`app-sash ${axis} ${className} ${dragging ? 'active' : ''}`}
            onPointerDown={onPointerDown}
            onPointerMove={onPointerMove}
            onLostPointerCapture={endDrag}
            onDoubleClick={() => onSizeChange(null)}
            onKeyDown={onKeyDown}
        />
    );
}
