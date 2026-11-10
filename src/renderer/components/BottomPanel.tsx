import type { ReactNode } from 'react';
import {
    BOTTOM_PANEL_MIN_HEIGHT,
    type BottomPanelState,
    bottomPanelMaxHeight,
    bottomPanelStyle,
    useDraftHeight,
} from '../hooks/useBottomPanel';
import { DocsPanel } from './DocsPanel';
import { Sash } from './Sash';
import './BottomPanel.css';

interface BottomPanelProps {
    state: BottomPanelState;
    /** Mounted only while the mixer shows, so its meters stop drawing when hidden. */
    mixer: ReactNode;
}

/** The panel under the editor, showing the mixer or the docs. */
export function BottomPanel({ state, mixer }: BottomPanelProps) {
    const { tab, docs } = state;
    const [height, setHeight] = useDraftHeight(
        state.height,
        state.commitHeight,
    );
    return (
        <section
            id="app-bottom-panel"
            className="app-bottom-panel"
            aria-label={tab === 'docs' ? 'Docs' : 'Mixer'}
            hidden={tab === null}
            style={bottomPanelStyle(height)}
        >
            <Sash
                panelId="app-bottom-panel"
                axis="y"
                grow={-1}
                min={BOTTOM_PANEL_MIN_HEIGHT}
                max={bottomPanelMaxHeight}
                label="Resize bottom panel"
                className="app-bottom-panel-sash"
                onSizeChange={setHeight}
            />
            {tab === 'mixer' && mixer}
            {/* Stays mounted so its page, search and expanded types survive tab switches. */}
            {docs.location === 'bottom' && (
                <DocsPanel target={docs.target} hidden={tab !== 'docs'} />
            )}
        </section>
    );
}
