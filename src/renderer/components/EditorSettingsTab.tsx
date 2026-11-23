import type { AppConfig, MonospaceFont } from '../../shared/ipcTypes';
import type { AppTheme } from '../themes/types';
import type { SidebarSide } from './sidebarLayout';
import {
    CURSOR_STYLES,
    DEFAULT_CURSOR_STYLE,
    type CursorStyle,
} from '../../shared/cursorStyle';
import { BUNDLED_FONTS } from '../../shared/fonts';
import { useInstalledSystemFonts } from './fonts';
import { RangeSetting, SelectSetting, ToggleSetting } from './SettingsControls';

interface EditorSettingsTabProps {
    config: AppConfig;
    themes: AppTheme[];
    onConfigChange: (partial: Partial<AppConfig>) => void;
}

export function EditorSettingsTab({
    config,
    themes,
    onConfigChange,
}: EditorSettingsTabProps) {
    const availableSystemFonts = useInstalledSystemFonts();

    return (
        <div className="settings-tab-content">
            <SelectSetting
                label="Color Theme"
                value={config.theme || 'modular-dark'}
                onChange={(value) => onConfigChange({ theme: value })}
            >
                {themes.map((t) => (
                    <option key={t.id} value={t.id}>
                        {t.name}
                    </option>
                ))}
            </SelectSetting>

            <SelectSetting
                label="Font"
                value={config.font || 'Fira Code'}
                onChange={(value) =>
                    onConfigChange({
                        font: value as MonospaceFont,
                    })
                }
            >
                <optgroup label="Bundled">
                    {BUNDLED_FONTS.map((f) => (
                        <option key={f} value={f}>
                            {f}
                        </option>
                    ))}
                </optgroup>
                {availableSystemFonts.length > 0 && (
                    <optgroup label="System">
                        {availableSystemFonts.map((f) => (
                            <option key={f} value={f}>
                                {f}
                            </option>
                        ))}
                    </optgroup>
                )}
            </SelectSetting>

            <RangeSetting
                label="Font Size"
                min={8}
                max={72}
                step={1}
                value={config.fontSize ?? 17}
                format={(value) => `${value}px`}
                onChange={(fontSize) => onConfigChange({ fontSize })}
            />

            <ToggleSetting
                label="Font Ligatures"
                checked={config.fontLigatures ?? false}
                onChange={(fontLigatures) => onConfigChange({ fontLigatures })}
            />

            <SelectSetting
                label="Cursor Style"
                value={config.cursorStyle ?? DEFAULT_CURSOR_STYLE}
                onChange={(value) =>
                    onConfigChange({
                        cursorStyle: value as CursorStyle,
                    })
                }
            >
                {CURSOR_STYLES.map((s) => (
                    <option key={s} value={s}>
                        {s}
                    </option>
                ))}
            </SelectSetting>

            <RangeSetting
                label="Code Backing Over Video"
                min={0}
                max={1}
                step={0.01}
                value={config.codeBackdropOpacity ?? 0.6}
                format={(value) => value.toFixed(2)}
                onChange={(codeBackdropOpacity) =>
                    onConfigChange({ codeBackdropOpacity })
                }
            />

            <SelectSetting
                label="Sidebar Side"
                value={config.sidebarSide ?? 'right'}
                onChange={(value) =>
                    onConfigChange({
                        sidebarSide: value as SidebarSide,
                    })
                }
            >
                <option value="left">left</option>
                <option value="right">right</option>
            </SelectSetting>

            <SelectSetting
                label="Docs Location"
                value={config.docsLocation ?? 'bottom'}
                onChange={(value) =>
                    onConfigChange({
                        docsLocation: value as 'bottom' | 'sidebar',
                    })
                }
            >
                <option value="bottom">bottom</option>
                <option value="sidebar">sidebar</option>
            </SelectSetting>
        </div>
    );
}
