import {
    Activity,
    BookOpen,
    Files,
    type LucideIcon,
    Settings,
    SlidersHorizontal,
} from 'lucide-react';
import { type ReactNode, useEffect, useLayoutEffect, useState } from 'react';
import { Group, Panel, Separator, usePanelRef } from 'react-resizable-panels';
import type { AppConfig } from '../../shared/ipcTypes';
import { registerCommand, unregisterCommand } from '../keybindings/commands';
import { DocsPanel } from './DocsPanel';
import type { DocsState } from '../hooks/useBottomPanel';
import { SettingsPanel } from './SettingsPanel';
import { useStoredPanelSize } from '../hooks/useStoredPanelSize';
import { useSidebarOpen, useSidebarSide } from './sidebarLayout';
import './Sidebar.css';

export type SidebarTab =
    'explorer' | 'control' | 'perform' | 'docs' | 'settings';

const OPEN_SETTINGS_COMMAND = 'shoreline.openSettings';
const WIDTH_STORAGE_KEY = 'modular_sidebar_width';
const SIDEBAR_DEFAULT_WIDTH = 344;
const SIDEBAR_MIN_WIDTH = 160;
/** Room kept for the editor column when the sidebar grows. */
const WORKSPACE_MIN_WIDTH = 240;

const TABS: { id: SidebarTab; label: string; icon: LucideIcon }[] = [
    { id: 'explorer', label: 'Explorer', icon: Files },
    { id: 'control', label: 'Control', icon: SlidersHorizontal },
    { id: 'perform', label: 'Perform', icon: Activity },
    { id: 'docs', label: 'Docs', icon: BookOpen },
    { id: 'settings', label: 'Settings', icon: Settings },
];

interface SidebarProps {
    /** The editor column the sidebar stands beside. */
    children: ReactNode;
    explorerContent: ReactNode;
    controlContent: ReactNode;
    performContent: ReactNode;
    /** The docs show as a tab here while their location is the sidebar. */
    docs: DocsState;
    /** The app's editable copy of the config file. */
    config: AppConfig;
    onConfigChange: (partial: Partial<AppConfig>) => void;
    /** False until the config file has been read. */
    configLoaded: boolean;
}

/**
 * The editor column beside a resizable sidebar panel, with the activity bar
 * on the window's outer edge. The panel's width is remembered across
 * launches.
 */
export function Sidebar({
    children,
    explorerContent,
    controlContent,
    performContent,
    docs,
    config,
    onConfigChange,
    configLoaded,
}: SidebarProps) {
    const [selectedTab, setSelectedTab] = useState<SidebarTab>('explorer');
    const [open, setOpen] = useSidebarOpen();
    const docsHere = docs.location === 'sidebar';
    const [seenReveals, setSeenReveals] = useState(docs.reveals);
    if (docs.reveals !== seenReveals) {
        setSeenReveals(docs.reveals);
        setSelectedTab('docs');
        setOpen(true);
    }
    const tabs = docsHere ? TABS : TABS.filter(({ id }) => id !== 'docs');
    const activeTab =
        selectedTab === 'docs' && !docsHere ? 'explorer' : selectedTab;
    const activeLabel = TABS.find(({ id }) => id === activeTab)?.label;
    const side = useSidebarSide(config.sidebarSide, configLoaded);
    const panelRef = usePanelRef();
    const width = useStoredPanelSize(
        WIDTH_STORAGE_KEY,
        SIDEBAR_DEFAULT_WIDTH,
        panelRef,
    );
    const restoreWidth = width.restore;

    // A closed panel stays mounted at zero width, so the panels inside keep
    // their state. Moving it to the other side lays the group
    // out afresh, so the width is applied again.
    useLayoutEffect(() => {
        if (open) {
            panelRef.current?.expand();
            restoreWidth();
        } else {
            panelRef.current?.collapse();
        }
    }, [open, side, panelRef, restoreWidth]);

    useEffect(() => {
        registerCommand(
            OPEN_SETTINGS_COMMAND,
            () => {
                setSelectedTab('settings');
                setOpen(true);
            },
            { label: 'Open Settings', category: 'Preferences' },
        );
        return () => {
            unregisterCommand(OPEN_SETTINGS_COMMAND);
        };
    }, [setOpen]);

    const shownTab = open ? activeTab : null;

    // Clicking the shown panel's button closes the sidebar, as in VS Code.
    const onActivityClick = (id: SidebarTab) => {
        setOpen(!open || id !== activeTab);
        setSelectedTab(id);
    };

    const sidebarPanel = (
        <Panel
            style={{ overflow: 'hidden' }}
            key="sidebar"
            id="sidebar"
            className="app-sidebar-panel-frame"
            panelRef={panelRef}
            defaultSize={width.defaultSize}
            minSize={SIDEBAR_MIN_WIDTH}
            collapsible
            // Dragging the panel shut closes it, as in VS Code.
            onResize={() => {
                if (open && panelRef.current?.isCollapsed()) setOpen(false);
            }}
            groupResizeBehavior="preserve-pixel-size"
        >
            <section
                id="app-sidebar-panel"
                className="app-sidebar-panel"
                aria-labelledby="app-sidebar-title"
                hidden={!open}
            >
                <h2 id="app-sidebar-title" className="app-sidebar-title">
                    {activeLabel}
                </h2>
                <div className="app-sidebar-content">
                    {activeTab !== 'docs' &&
                        activeTab !== 'settings' &&
                        {
                            control: controlContent,
                            explorer: explorerContent,
                            perform: performContent,
                        }[activeTab]}
                    {/* Stays mounted so its page, search and expanded types survive tab switches. */}
                    {docsHere && (
                        <DocsPanel
                            target={docs.target}
                            hidden={shownTab !== 'docs'}
                        />
                    )}
                    <SettingsPanel
                        hidden={shownTab !== 'settings'}
                        config={config}
                        onConfigChange={onConfigChange}
                    />
                </div>
            </section>
        </Panel>
    );
    const sash = open && (
        <Separator
            key="sash"
            className={`app-sash ${side === 'right' ? 'into-next' : 'into-previous'}`}
            aria-label="Resize sidebar"
        />
    );
    const workspace = (
        <Panel
            style={{ overflow: 'hidden' }}
            key="workspace"
            id="workspace"
            className="app-sidebar-workspace"
            minSize={WORKSPACE_MIN_WIDTH}
        >
            {children}
        </Panel>
    );

    return (
        <div className={`app-sidebar ${side} ${open ? '' : 'collapsed'}`}>
            <Group
                className="app-sidebar-group"
                orientation="horizontal"
                onLayoutChanged={width.onLayoutChanged}
            >
                {side === 'left'
                    ? [sidebarPanel, sash, workspace]
                    : [workspace, sash, sidebarPanel]}
            </Group>
            <nav className="app-activity-bar" aria-label="Sidebar panels">
                {tabs.map(({ id, label, icon: Icon }) => {
                    const shown = shownTab === id;
                    return (
                        <button
                            key={id}
                            aria-pressed={shown}
                            aria-controls="app-sidebar-panel"
                            aria-label={label}
                            title={label}
                            className={`app-activity-bar-item ${id} ${shown ? 'active' : ''}`}
                            onClick={() => onActivityClick(id)}
                        >
                            <Icon size={22} strokeWidth={1.5} aria-hidden />
                        </button>
                    );
                })}
            </nav>
        </div>
    );
}
