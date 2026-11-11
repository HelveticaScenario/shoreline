import {
    Activity,
    BookOpen,
    Files,
    type LucideIcon,
    Settings,
    SlidersHorizontal,
} from 'lucide-react';
import { type ReactNode, useEffect, useState } from 'react';
import type { AppConfig } from '../../shared/ipcTypes';
import { registerCommand, unregisterCommand } from '../keybindings/commands';
import { DocsPanel } from './DocsPanel';
import type { DocsState } from '../hooks/useBottomPanel';
import { Sash } from './Sash';
import { SettingsPanel } from './SettingsPanel';
import {
    SIDEBAR_MIN_WIDTH,
    sidebarMaxWidth,
    sidebarPanelStyle,
    useSidebarOpen,
    useSidebarSide,
    useSidebarWidth,
} from './sidebarLayout';
import './Sidebar.css';

export type SidebarTab =
    | 'explorer'
    | 'control'
    | 'perform'
    | 'docs'
    | 'settings';

const OPEN_SETTINGS_COMMAND = 'shoreline.openSettings';

const TABS: { id: SidebarTab; label: string; icon: LucideIcon }[] = [
    { id: 'explorer', label: 'Explorer', icon: Files },
    { id: 'control', label: 'Control', icon: SlidersHorizontal },
    { id: 'perform', label: 'Perform', icon: Activity },
    { id: 'docs', label: 'Docs', icon: BookOpen },
    { id: 'settings', label: 'Settings', icon: Settings },
];

interface SidebarProps {
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

export function Sidebar({
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
    const [width, setWidth] = useSidebarWidth();
    const side = useSidebarSide(config.sidebarSide, configLoaded);

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

    return (
        <div className={`app-sidebar ${side} ${open ? '' : 'collapsed'}`}>
            {open && (
                <Sash
                    panelId="app-sidebar-panel"
                    axis="x"
                    grow={side === 'right' ? -1 : 1}
                    min={SIDEBAR_MIN_WIDTH}
                    max={sidebarMaxWidth}
                    label="Resize sidebar"
                    className="app-sidebar-sash"
                    onSizeChange={setWidth}
                />
            )}
            {/* Stays mounted while closed so panels keep their state. */}
            <section
                id="app-sidebar-panel"
                className="app-sidebar-panel"
                aria-labelledby="app-sidebar-title"
                style={sidebarPanelStyle(width)}
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
