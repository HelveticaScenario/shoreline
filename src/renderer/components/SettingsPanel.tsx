import { useEffect, useState } from 'react';
import { useTheme } from '../themes/ThemeContext';
import { AudioSettingsTab } from './AudioSettings';
import { EditorSettingsTab } from './EditorSettingsTab';
import { FormatterSettingsTab } from './FormatterSettingsTab';
import { bindSettingsVisible } from '../keybindings/contextKeyBootstrap';
import type { AppConfig } from '../../shared/ipcTypes';
import './Settings.css';

type SettingsTabId = 'editor' | 'audio' | 'formatter';

const TABS: { id: SettingsTabId; label: string }[] = [
    { id: 'editor', label: 'Editor' },
    { id: 'audio', label: 'Audio' },
    { id: 'formatter', label: 'Formatter' },
];

interface SettingsPanelProps {
    hidden: boolean;
    config: AppConfig;
    onConfigChange: (partial: Partial<AppConfig>) => void;
}

/** The app's settings in the sidebar. Editor and formatter changes save as they are made. */
export function SettingsPanel({
    hidden,
    config,
    onConfigChange,
}: SettingsPanelProps) {
    const [activeTab, setActiveTab] = useState<SettingsTabId>('editor');
    const { themes } = useTheme();

    useEffect(() => bindSettingsVisible(!hidden), [hidden]);

    return (
        <div className="settings-sidebar" hidden={hidden}>
            <div className="settings-tabs" role="tablist">
                {TABS.map((tab) => (
                    <button
                        key={tab.id}
                        role="tab"
                        aria-selected={activeTab === tab.id}
                        className={`settings-tab-btn${activeTab === tab.id ? ' active' : ''}`}
                        onClick={() => setActiveTab(tab.id)}
                    >
                        {tab.label}
                    </button>
                ))}
            </div>

            <div className="settings-body">
                {activeTab === 'editor' && (
                    <EditorSettingsTab
                        config={config}
                        themes={themes}
                        onConfigChange={onConfigChange}
                    />
                )}
                {activeTab === 'audio' && (
                    <AudioSettingsTab isActive={!hidden} />
                )}
                {activeTab === 'formatter' && (
                    <FormatterSettingsTab
                        config={config}
                        onConfigChange={onConfigChange}
                    />
                )}
            </div>
        </div>
    );
}
