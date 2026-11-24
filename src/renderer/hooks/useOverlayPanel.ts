import { type RefObject, useEffect, useEffectEvent, useRef } from 'react';

/**
 * The behaviour of a panel shown over the app while `isOpen`: the element
 * given the returned ref takes focus when the panel opens, and Escape calls
 * `onClose`.
 */
export function useOverlayPanel(
    isOpen: boolean,
    onClose: () => void,
): RefObject<HTMLDivElement | null> {
    const panelRef = useRef<HTMLDivElement>(null);

    useEffect(() => {
        if (!isOpen) return;
        const rafId = requestAnimationFrame(() => {
            panelRef.current?.focus();
        });
        return () => cancelAnimationFrame(rafId);
    }, [isOpen]);

    const onEscape = useEffectEvent(onClose);
    useEffect(() => {
        if (!isOpen) return;
        const handleKeyDown = (e: KeyboardEvent) => {
            if (e.key === 'Escape') onEscape();
        };
        window.addEventListener('keydown', handleKeyDown);
        return () => window.removeEventListener('keydown', handleKeyDown);
    }, [isOpen]);

    return panelRef;
}
