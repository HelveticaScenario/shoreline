import {
    type CSSProperties,
    useCallback,
    useEffect,
    useRef,
    useState,
} from 'react';
import { registerCommand, unregisterCommand } from '../keybindings/commands';
import {
    type DocsTarget,
    SHOW_DOCS_COMMAND,
    toDocsTarget,
} from '../components/docsCommand';
import type { AppConfig } from '../../shared/ipcTypes';

export type BottomTab = 'mixer' | 'docs';

export const BOTTOM_PANEL_MIN_HEIGHT = 84;
const DEFAULT_HEIGHT = 200;
/** Room kept for the header, the editor and the bottom bar when the panel grows. */
const RESERVED_HEIGHT = 240;
/** How long a height must hold before it is written to the config file. */
const HEIGHT_WRITE_DELAY_MS = 300;

export function bottomPanelMaxHeight(): number {
    return Math.max(
        BOTTOM_PANEL_MIN_HEIGHT,
        window.innerHeight - RESERVED_HEIGHT,
    );
}

/** The panel's sizing. A short window squeezes the panel before the editor. */
export function bottomPanelStyle(height: number): CSSProperties {
    return {
        height,
        maxHeight: `max(${BOTTOM_PANEL_MIN_HEIGHT}px, calc(100vh - ${RESERVED_HEIGHT}px))`,
    };
}

/** Where the docs live and what they were last asked to show. */
export interface DocsState {
    /** `null` until the config file is read, so the docs mount only once, in their place. */
    location: 'bottom' | 'sidebar' | null;
    target: DocsTarget | null;
    /** Counts the requests to bring the docs forward. */
    reveals: number;
}

export interface BottomPanelState {
    /** The open panel, or `null` while collapsed to the bar. */
    tab: BottomTab | null;
    /** Opens `tab`, or collapses the panel when `tab` is already open. */
    toggle: (tab: BottomTab) => void;
    show: (tab: BottomTab | null) => void;
    /** The height kept in the config file. */
    height: number;
    commitHeight: (height: number) => void;
    docs: DocsState;
}

/**
 * The panel's height while a drag runs, ahead of the config file, and the
 * setter the sash drives. A height is written once it holds still, and a
 * pending write is flushed on unmount. Kept beside the panel so a drag
 * re-renders only the panel.
 */
export function useDraftHeight(
    committed: number,
    commit: (height: number) => void,
): [number, (height: number | null) => void] {
    const [draft, setDraft] = useState<number | null>(null);
    const pending = useRef<{ timer: number; height: number } | null>(null);
    const commitRef = useRef(commit);
    useEffect(() => {
        commitRef.current = commit;
    }, [commit]);

    const setHeight = useCallback((next: number | null) => {
        const height = next ?? DEFAULT_HEIGHT;
        setDraft(height);
        if (pending.current) window.clearTimeout(pending.current.timer);
        const timer = window.setTimeout(() => {
            pending.current = null;
            commitRef.current(height);
            setDraft(null);
        }, HEIGHT_WRITE_DELAY_MS);
        pending.current = { timer, height };
    }, []);

    useEffect(
        () => () => {
            const write = pending.current;
            if (!write) return;
            window.clearTimeout(write.timer);
            commitRef.current(write.height);
        },
        [],
    );

    return [draft ?? committed, setHeight];
}

/**
 * Which panel shows under the editor and how tall it is, both kept in the
 * config file. Registers the commands that open the mixer and the docs,
 * wherever the docs live.
 */
export function useBottomPanel(
    config: AppConfig,
    changeConfig: (partial: Partial<AppConfig>) => void,
    configLoaded: boolean,
): BottomPanelState {
    const docsLocation = configLoaded
        ? (config.docsLocation ?? 'bottom')
        : null;
    const storedTab = config.bottomPanel ?? null;
    const tab =
        docsLocation === 'sidebar' && storedTab === 'docs' ? null : storedTab;
    const height = config.bottomPanelHeight ?? DEFAULT_HEIGHT;
    const [docsTarget, setDocsTarget] = useState<DocsTarget | null>(null);
    const [reveals, setReveals] = useState(0);
    // The docs mount afresh in their new place and show a target on mount,
    // so a move forgets the last one.
    const [targetLocation, setTargetLocation] = useState(docsLocation);
    if (docsLocation !== targetLocation) {
        setTargetLocation(docsLocation);
        setDocsTarget(null);
    }

    // The commands are registered once and read the latest values here.
    const current = useRef({ tab, docsLocation });
    useEffect(() => {
        current.current = { tab, docsLocation };
    }, [tab, docsLocation]);

    // An undefined field survives IPC and clears the key from the file.
    const show = useCallback(
        (next: BottomTab | null) =>
            changeConfig({ bottomPanel: next ?? undefined }),
        [changeConfig],
    );

    const toggle = useCallback(
        (next: BottomTab) => show(current.current.tab === next ? null : next),
        [show],
    );

    const commitHeight = useCallback(
        (next: number) => changeConfig({ bottomPanelHeight: next }),
        [changeConfig],
    );

    useEffect(() => {
        // Before the config file is read the docs have no place to open in.
        const bringDocsForward = (togglesAtBottom: boolean) => {
            const location = current.current.docsLocation;
            if (location === 'sidebar') setReveals((n) => n + 1);
            else if (location === 'bottom') {
                if (togglesAtBottom) toggle('docs');
                else show('docs');
            }
        };
        registerCommand('shoreline.toggleMixer', () => toggle('mixer'), {
            label: 'Toggle Mixer',
            category: 'View',
        });
        registerCommand('shoreline.toggleDocs', () => bringDocsForward(true), {
            label: 'Toggle Docs',
            category: 'View',
        });
        registerCommand(
            SHOW_DOCS_COMMAND,
            (target) => {
                setDocsTarget(toDocsTarget(target));
                bringDocsForward(false);
            },
            { label: 'Show Docs', category: 'View' },
        );
        return () => {
            unregisterCommand('shoreline.toggleMixer');
            unregisterCommand('shoreline.toggleDocs');
            unregisterCommand(SHOW_DOCS_COMMAND);
        };
    }, [show, toggle]);

    return {
        tab,
        toggle,
        show,
        height,
        commitHeight,
        docs: { location: docsLocation, target: docsTarget, reveals },
    };
}
