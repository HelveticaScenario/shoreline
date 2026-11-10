import {
    Activity,
    BookOpen,
    Files,
    type LucideIcon,
    Settings,
    SlidersHorizontal,
} from 'lucide-react';
import { type ReactNode, useEffect, useState } from 'react';
import { useEditableConfig } from '../hooks/useEditableConfig';
import { registerCommand, unregisterCommand } from '../keybindings/commands';
import { DocsPanel } from './DocsPanel';
import {
    type DocsTarget,
    SHOW_DOCS_COMMAND,
    toDocsTarget,
} from './docsCommand';
import { SettingsPanel } from './SettingsPanel';
import {
    SidebarSash,
    sidebarPanelStyle,
    useSidebarOpen,
    useSidebarSide,
    useSidebarWidth,
} from './SidebarSash';
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
}

export function Sidebar({
    explorerContent,
    controlContent,
    performContent,
}: SidebarProps) {
    const [activeTab, setActiveTab] = useState<SidebarTab>('explorer');
    const [open, setOpen] = useSidebarOpen();
    const activeLabel = TABS.find(({ id }) => id === activeTab)?.label;
    const [width, setWidth] = useSidebarWidth();
    const [docsTarget, setDocsTarget] = useState<DocsTarget | null>(null);
    const [config, changeConfig, configLoaded] = useEditableConfig(true);
    const side = useSidebarSide(config.sidebarSide, configLoaded);

    useEffect(() => {
        registerCommand(
            SHOW_DOCS_COMMAND,
            (target) => {
                setActiveTab('docs');
                setOpen(true);
                setDocsTarget(toDocsTarget(target));
            },
            { label: 'Show Docs', category: 'View' },
        );
        registerCommand(
            OPEN_SETTINGS_COMMAND,
            () => {
                setActiveTab('settings');
                setOpen(true);
            },
            { label: 'Open Settings', category: 'Preferences' },
        );
        return () => {
            unregisterCommand(SHOW_DOCS_COMMAND);
            unregisterCommand(OPEN_SETTINGS_COMMAND);
        };
    }, []);

    // Clicking the shown panel's button closes the sidebar, as in VS Code.
    const onActivityClick = (id: SidebarTab) => {
        setOpen(!open || id !== activeTab);
        setActiveTab(id);
    };

    return (
        <div className={`app-sidebar ${side} ${open ? '' : 'collapsed'}`}>
            {open && (
                <SidebarSash
                    panelId="app-sidebar-panel"
                    side={side}
                    onWidthChange={setWidth}
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
                    <DocsPanel
                        target={docsTarget}
                        hidden={activeTab !== 'docs'}
                    />
                    <SettingsPanel
                        hidden={activeTab !== 'settings'}
                        config={config}
                        onConfigChange={changeConfig}
                    />
                </div>
            </section>
            <nav className="app-activity-bar" aria-label="Sidebar panels">
                {TABS.map(({ id, label, icon: Icon }) => {
                    const shown = open && activeTab === id;
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
