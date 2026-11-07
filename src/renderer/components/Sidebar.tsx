import {
    Activity,
    Files,
    type LucideIcon,
    SlidersHorizontal,
} from 'lucide-react';
import { type ReactNode, useState } from 'react';
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

    return (
        <div className="app-sidebar">
            <div className="app-sidebar-panel">
                <div className="app-sidebar-title">{activeLabel}</div>
                <div className="app-sidebar-content">
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
            >
                {TABS.map(({ id, label, icon: Icon }) => (
                    <button
                        key={id}
                        role="tab"
                        aria-selected={activeTab === id}
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
