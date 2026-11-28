import { type ReactNode, useLayoutEffect } from 'react';
import {
    Group,
    type LayoutChangedMeta,
    Panel,
    Separator,
    usePanelRef,
} from 'react-resizable-panels';
import {
    BOTTOM_PANEL_DEFAULT_HEIGHT,
    BOTTOM_PANEL_MIN_HEIGHT,
    type BottomPanelState,
} from '../hooks/useBottomPanel';
import { DocsPanel } from './DocsPanel';
import './BottomPanel.css';

/** Room kept for the editor when the panel grows. */
const EDITOR_MIN_HEIGHT = 160;

interface BottomPanelProps {
    state: BottomPanelState;
    /** Mounted only while the mixer shows, so its meters stop drawing when hidden. */
    mixer: ReactNode;
    /** The editor the panel sits under. */
    children: ReactNode;
}

/** The editor with the panel under it, showing the mixer or the docs. */
export function BottomPanel({ state, mixer, children }: BottomPanelProps) {
    const { tab, docs, height, commitHeight, show } = state;
    const open = tab !== null;
    const panelRef = usePanelRef();

    // The config file holds the height, so the panel follows it there. A
    // closed panel stays mounted at zero height, so the docs keep their state.
    useLayoutEffect(() => {
        const panel = panelRef.current;
        if (!panel) return;
        if (!open) panel.collapse();
        else if (Math.round(panel.getSize().inPixels) !== height)
            panel.resize(height);
    }, [open, height, panelRef]);

    // Fires once a drag is released, and on each key press. The new layout
    // reaches the DOM after this callback, so the height is read on the next
    // frame.
    const onLayoutChanged = (_: unknown, meta: LayoutChangedMeta) => {
        if (!meta.isUserInteraction) return;
        requestAnimationFrame(() => {
            const panel = panelRef.current;
            if (!panel || panel.isCollapsed()) return;
            commitHeight(Math.round(panel.getSize().inPixels));
        });
    };

    return (
        <Group
            className="app-workspace"
            orientation="vertical"
            onLayoutChanged={onLayoutChanged}
        >
            <Panel
                style={{ overflow: 'hidden' }}
                id="editor"
                className="app-workspace-editor"
                minSize={EDITOR_MIN_HEIGHT}
            >
                {children}
            </Panel>
            {open && (
                <Separator
                    className="app-sash into-next"
                    aria-label="Resize bottom panel"
                />
            )}
            <Panel
                style={{ overflow: 'hidden' }}
                id="bottom-panel"
                className="app-bottom-panel-frame"
                panelRef={panelRef}
                defaultSize={BOTTOM_PANEL_DEFAULT_HEIGHT}
                minSize={BOTTOM_PANEL_MIN_HEIGHT}
                collapsible
                // Dragging the panel shut closes it.
                onResize={() => {
                    if (open && panelRef.current?.isCollapsed()) show(null);
                }}
                groupResizeBehavior="preserve-pixel-size"
            >
                <section
                    id="app-bottom-panel"
                    className="app-bottom-panel"
                    aria-label={tab === 'docs' ? 'Docs' : 'Mixer'}
                    hidden={!open}
                >
                    {tab === 'mixer' && mixer}
                    {/* Stays mounted so its page, search and expanded types survive tab switches. */}
                    {docs.location === 'bottom' && (
                        <DocsPanel
                            target={docs.target}
                            hidden={tab !== 'docs'}
                        />
                    )}
                </section>
            </Panel>
        </Group>
    );
}
