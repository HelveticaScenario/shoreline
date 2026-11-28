import { useEffect, useEffectEvent, useState } from 'react';
import { registerCommand, unregisterCommand } from '../keybindings/commands';
import {
    type DocsTarget,
    SHOW_DOCS_COMMAND,
    toDocsTarget,
} from '../components/docsCommand';
import type { AppConfig } from '../../shared/ipcTypes';

export type BottomTab = 'mixer' | 'docs';

export const BOTTOM_PANEL_MIN_HEIGHT = 84;
export const BOTTOM_PANEL_DEFAULT_HEIGHT = 200;

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
    const height = config.bottomPanelHeight ?? BOTTOM_PANEL_DEFAULT_HEIGHT;
    const [docsTarget, setDocsTarget] = useState<DocsTarget | null>(null);
    const [reveals, setReveals] = useState(0);
    // The docs mount afresh in their new place and show a target on mount,
    // so a move forgets the last one.
    const [targetLocation, setTargetLocation] = useState(docsLocation);
    if (docsLocation !== targetLocation) {
        setTargetLocation(docsLocation);
        setDocsTarget(null);
    }

    // An undefined field survives IPC and clears the key from the file.
    const show = (next: BottomTab | null) =>
        changeConfig({ bottomPanel: next ?? undefined });

    const toggle = (next: BottomTab) => show(tab === next ? null : next);

    const commitHeight = (next: number) =>
        changeConfig({ bottomPanelHeight: next });

    // The commands are registered once and read the latest values through these.
    const toggleMixer = useEffectEvent(() => toggle('mixer'));
    // Before the config file is read the docs have no place to open in.
    const bringDocsForward = useEffectEvent((togglesAtBottom: boolean) => {
        if (docsLocation === 'sidebar') setReveals((n) => n + 1);
        else if (docsLocation === 'bottom') {
            if (togglesAtBottom) toggle('docs');
            else show('docs');
        }
    });

    useEffect(() => {
        registerCommand('shoreline.toggleMixer', () => toggleMixer(), {
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
    }, []);

    return {
        tab,
        toggle,
        show,
        height,
        commitHeight,
        docs: { location: docsLocation, target: docsTarget, reveals },
    };
}
