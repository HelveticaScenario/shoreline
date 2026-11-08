import {
    Activity,
    BookOpen,
    Files,
    type LucideIcon,
    SlidersHorizontal,
} from 'lucide-react';
import { type ReactNode, useEffect, useState } from 'react';
import { registerCommand, unregisterCommand } from '../keybindings/commands';
import { DocsPanel } from './DocsPanel';
import {
    type DocsTarget,
    SHOW_DOCS_COMMAND,
    toDocsTarget,
} from './docsCommand';
import { SidebarSash, sidebarPanelStyle, useSidebarWidth } from './SidebarSash';
import './Sidebar.css';

export type SidebarTab = 'explorer' | 'control' | 'perform' | 'docs';

const TABS: { id: SidebarTab; label: string; icon: LucideIcon }[] = [
    { id: 'explorer', label: 'Explorer', icon: Files },
    { id: 'control', label: 'Control', icon: SlidersHorizontal },
    { id: 'perform', label: 'Perform', icon: Activity },
    { id: 'docs', label: 'Docs', icon: BookOpen },
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
    const activeLabel = TABS.find(({ id }) => id === activeTab)?.label;
    const [width, setWidth] = useSidebarWidth();
    const [docsTarget, setDocsTarget] = useState<DocsTarget | null>(null);

    useEffect(() => {
        registerCommand(
            SHOW_DOCS_COMMAND,
            (target) => {
                setActiveTab('docs');
                setDocsTarget(toDocsTarget(target));
            },
            { label: 'Show Docs', category: 'View' },
        );
        return () => {
            unregisterCommand(SHOW_DOCS_COMMAND);
        };
    }, []);

    return (
        <div className="app-sidebar">
            <SidebarSash panelId="app-sidebar-panel" onWidthChange={setWidth} />
            <section
                id="app-sidebar-panel"
                className="app-sidebar-panel"
                aria-labelledby="app-sidebar-title"
                style={sidebarPanelStyle(width)}
            >
                <h2 id="app-sidebar-title" className="app-sidebar-title">
                    {activeLabel}
                </h2>
                <div className="app-sidebar-content">
                    {activeTab !== 'docs' &&
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
                </div>
            </section>
            <nav className="app-activity-bar" aria-label="Sidebar panels">
                {TABS.map(({ id, label, icon: Icon }) => (
                    <button
                        key={id}
                        aria-pressed={activeTab === id}
                        aria-label={label}
                        title={label}
                        className={`app-activity-bar-item ${activeTab === id ? 'active' : ''}`}
                        onClick={() => setActiveTab(id)}
                    >
                        <Icon size={22} strokeWidth={1.5} aria-hidden />
                    </button>
                ))}
            </nav>
        </div>
    );
}
