import { useEffect, useState, type RefObject } from 'react';

/** How far outside its scroll container an element counts as on screen. */
const MARGIN = '200px 0px';

const listeners = new Map<Element, (near: boolean) => void>();
let observer: IntersectionObserver | null = null;

/** One observer for every element, which costs far less than one each. */
function sharedObserver(): IntersectionObserver {
    observer ??= new IntersectionObserver(
        (entries) => {
            for (const entry of entries) {
                listeners.get(entry.target)?.(entry.isIntersecting);
            }
        },
        // `scrollMargin` widens every scroll container the element sits in;
        // `rootMargin` would widen only the window.
        { scrollMargin: MARGIN } as IntersectionObserverInit,
    );
    return observer;
}

/** Whether `ref`'s element is on screen, or about to be. */
export function useNearScreen(ref: RefObject<Element | null>): boolean {
    const [near, setNear] = useState(false);
    useEffect(() => {
        const element = ref.current;
        if (!element) {
            return;
        }
        const io = sharedObserver();
        listeners.set(element, setNear);
        io.observe(element);
        return () => {
            io.unobserve(element);
            listeners.delete(element);
        };
    }, [ref]);
    return near;
}
