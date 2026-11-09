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
import { useEditableConfig } from './useEditableConfig';

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
    location: 'bottom' | 'sidebar';
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
    height: number;
    /** `null` restores the default height. */
    setHeight: (height: number | null) => void;
    docs: DocsState;
}

/**
 * Which panel shows under the editor and how tall it is, both kept in the
 * config file. Registers the commands that open the mixer and the docs,
 * wherever the docs live.
 */
export function useBottomPanel(): BottomPanelState {
    const [config, changeConfig] = useEditableConfig(true);
    const docsLocation = config.docsLocation ?? 'bottom';
    const storedTab = config.bottomPanel ?? null;
    const tab =
        docsLocation === 'sidebar' && storedTab === 'docs' ? null : storedTab;
    // The height while a drag runs, ahead of the config file.
    const [draftHeight, setDraftHeight] = useState<number | null>(null);
    const height = draftHeight ?? config.bottomPanelHeight ?? DEFAULT_HEIGHT;
    const [docsTarget, setDocsTarget] = useState<DocsTarget | null>(null);
    const [reveals, setReveals] = useState(0);

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

    const heightWrite = useRef<number>(undefined);
    const setHeight = useCallback(
        (next: number | null) => {
            const settled = next ?? DEFAULT_HEIGHT;
            setDraftHeight(settled);
            window.clearTimeout(heightWrite.current);
            heightWrite.current = window.setTimeout(() => {
                changeConfig({ bottomPanelHeight: settled });
                setDraftHeight(null);
            }, HEIGHT_WRITE_DELAY_MS);
        },
        [changeConfig],
    );
    useEffect(() => () => window.clearTimeout(heightWrite.current), []);

    useEffect(() => {
        const inSidebar = () => current.current.docsLocation === 'sidebar';
        const revealInSidebar = () => setReveals((n) => n + 1);
        registerCommand('shoreline.toggleMixer', () => toggle('mixer'), {
            label: 'Toggle Mixer',
            category: 'View',
        });
        registerCommand(
            'shoreline.toggleDocs',
            () => (inSidebar() ? revealInSidebar() : toggle('docs')),
            { label: 'Toggle Docs', category: 'View' },
        );
        registerCommand(
            SHOW_DOCS_COMMAND,
            (target) => {
                setDocsTarget(toDocsTarget(target));
                if (inSidebar()) revealInSidebar();
                else show('docs');
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
        setHeight,
        docs: { location: docsLocation, target: docsTarget, reveals },
    };
}
