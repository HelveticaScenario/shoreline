import {
    Activity,
    Files,
    type LucideIcon,
    SlidersHorizontal,
} from 'lucide-react';
import { type KeyboardEvent, type ReactNode, useState } from 'react';
import './Sidebar.css';

export type SidebarTab = 'explorer' | 'control' | 'perform';

const TABS: { id: SidebarTab; label: string; icon: LucideIcon }[] = [
    { id: 'explorer', label: 'Explorer', icon: Files },
    { id: 'control', label: 'Control', icon: SlidersHorizontal },
    { id: 'perform', label: 'Perform', icon: Activity },
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

    const handleKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
        const index = TABS.findIndex(({ id }) => id === activeTab);
        const next = {
            ArrowDown: (index + 1) % TABS.length,
            ArrowUp: (index - 1 + TABS.length) % TABS.length,
            Home: 0,
            End: TABS.length - 1,
        }[event.key];
        if (next === undefined) return;
        event.preventDefault();
        setActiveTab(TABS[next].id);
        document.getElementById(`sidebar-tab-${TABS[next].id}`)?.focus();
    };

    return (
        <div className="app-sidebar">
            <div className="app-sidebar-panel">
                <div className="app-sidebar-title">{activeLabel}</div>
                <div
                    className="app-sidebar-content"
                    role="tabpanel"
                    id="sidebar-tabpanel"
                    aria-labelledby={`sidebar-tab-${activeTab}`}
                >
                    {
                        {
                            control: controlContent,
                            explorer: explorerContent,
                            perform: performContent,
                        }[activeTab]
                    }
                </div>
            </div>
            <div
                className="app-activity-bar"
                role="tablist"
                aria-orientation="vertical"
                onKeyDown={handleKeyDown}
            >
                {TABS.map(({ id, label, icon: Icon }) => (
                    <button
                        key={id}
                        id={`sidebar-tab-${id}`}
                        role="tab"
                        aria-selected={activeTab === id}
                        aria-controls="sidebar-tabpanel"
                        tabIndex={activeTab === id ? 0 : -1}
                        aria-label={label}
                        title={label}
                        className={`app-activity-bar-item ${activeTab === id ? 'active' : ''}`}
                        onClick={() => setActiveTab(id)}
                    >
                        <Icon size={22} strokeWidth={1.5} aria-hidden />
                    </button>
                ))}
            </div>
        </div>
    );
}
