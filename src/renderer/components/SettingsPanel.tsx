import { useCallback, useEffect, useRef, useState } from 'react';
import { useTheme } from '../themes/ThemeContext';
import { type AudioSettingsHandle, AudioSettingsTab } from './AudioSettings';
import { EditorSettingsTab } from './EditorSettingsTab';
import { FormatterSettingsTab } from './FormatterSettingsTab';
import { bindSettingsVisible } from '../keybindings/contextKeyBootstrap';
import { useEditableConfig } from '../hooks/useEditableConfig';
import './Settings.css';

type SettingsTabId = 'editor' | 'audio' | 'formatter';

const TABS: { id: SettingsTabId; label: string }[] = [
    { id: 'editor', label: 'Editor' },
    { id: 'audio', label: 'Audio' },
    { id: 'formatter', label: 'Formatter' },
];

interface SettingsPanelProps {
    hidden: boolean;
}

/**
 * The app's settings in the sidebar. Editor and formatter changes save as
 * they are made; audio device changes wait for Apply, since reopening the
 * streams interrupts the sound.
 */
export function SettingsPanel({ hidden }: SettingsPanelProps) {
    const [activeTab, setActiveTab] = useState<SettingsTabId>('editor');
    const [config, change] = useEditableConfig(!hidden);
    const [applying, setApplying] = useState(false);
    const { themes } = useTheme();
    const audioRef = useRef<AudioSettingsHandle>(null);

    const applyAudio = useCallback(async () => {
        setApplying(true);
        try {
            await audioRef.current?.apply();
        } catch (err) {
            console.error('Failed to apply audio settings:', err);
        } finally {
            setApplying(false);
        }
    }, []);

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
                        onConfigChange={change}
                    />
                )}
                {activeTab === 'audio' && (
                    <AudioSettingsTab isActive={!hidden} ref={audioRef} />
                )}
                {activeTab === 'formatter' && (
                    <FormatterSettingsTab
                        config={config}
                        onConfigChange={change}
                    />
                )}
            </div>

            {activeTab === 'audio' && (
                <div className="settings-footer">
                    <button
                        className="btn btn-primary"
                        onClick={applyAudio}
                        disabled={applying}
                    >
                        {applying ? 'Applying…' : 'Apply'}
                    </button>
                </div>
            )}
        </div>
    );
}
