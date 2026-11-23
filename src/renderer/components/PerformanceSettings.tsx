import type { AppConfig, MonospaceFont } from '../../shared/ipcTypes';
import {
    DEFAULT_PERFORMANCE_VIEW,
    scrollEaseMs,
} from '../performance/settings';
import type { AppTheme } from '../themes/types';
import { RangeSetting, SelectSetting, ToggleSetting } from './SettingsControls';

interface PerformanceSettingsProps {
    config: AppConfig;
    themes: AppTheme[];
    /** The font options, grouped as the editor's font menu groups them. */
    fontGroups: { label: string; fonts: readonly string[] }[];
    onConfigChange: (partial: Partial<AppConfig>) => void;
}

/**
 * The performance window's settings: its shape and resolution, and how the
 * code looks there, apart from the editor's own look.
 */
export function PerformanceSettings({
    config,
    themes,
    fontGroups,
    onConfigChange,
}: PerformanceSettingsProps) {
    const d = DEFAULT_PERFORMANCE_VIEW;
    const fixed = (digits: number) => (value: number) => value.toFixed(digits);
    return (
        <>
            <SelectSetting
                label="Window Shape"
                value={config.performanceAspect ?? 'free'}
                onChange={(value) =>
                    onConfigChange({
                        performanceAspect: value as NonNullable<
                            AppConfig['performanceAspect']
                        >,
                    })
                }
            >
                <option value="free">Free</option>
                <option value="16:9">16:9</option>
                <option value="4:3">4:3</option>
                <option value="1:1">1:1</option>
                <option value="9:16">9:16</option>
            </SelectSetting>

            <SelectSetting
                label="Window Resolution"
                value={String(config.performanceScale ?? 1)}
                onChange={(value) =>
                    onConfigChange({
                        performanceScale: Number(value),
                    })
                }
            >
                <option value="1">100%</option>
                <option value="0.75">75%</option>
                <option value="0.5">50%</option>
                <option value="0.25">25%</option>
            </SelectSetting>

            <ToggleSetting
                label="Show Code"
                checked={config.performanceCodeVisible ?? d.codeVisible}
                onChange={(performanceCodeVisible) =>
                    onConfigChange({ performanceCodeVisible })
                }
            />
            <RangeSetting
                label="Code Opacity"
                min={0}
                max={1}
                step={0.01}
                value={config.performanceCodeOpacity ?? d.codeOpacity}
                format={fixed(2)}
                onChange={(performanceCodeOpacity) =>
                    onConfigChange({ performanceCodeOpacity })
                }
            />
            <RangeSetting
                label="Code Backing"
                min={0}
                max={1}
                step={0.01}
                value={config.performanceCodeBacking ?? d.backing}
                format={fixed(2)}
                onChange={(performanceCodeBacking) =>
                    onConfigChange({ performanceCodeBacking })
                }
            />

            <SelectSetting
                label="Code Theme"
                value={config.performanceTheme ?? ''}
                onChange={(value) =>
                    onConfigChange({
                        performanceTheme: value || undefined,
                    })
                }
            >
                <option value="">Same as editor</option>
                {themes.map((t) => (
                    <option key={t.id} value={t.id}>
                        {t.name}
                    </option>
                ))}
            </SelectSetting>

            <SelectSetting
                label="Code Font"
                value={config.performanceFont ?? d.font}
                onChange={(value) =>
                    onConfigChange({
                        performanceFont: value as MonospaceFont,
                    })
                }
            >
                {fontGroups
                    .filter(({ fonts }) => fonts.length > 0)
                    .map(({ label, fonts }) => (
                        <optgroup key={label} label={label}>
                            {fonts.map((f) => (
                                <option key={f} value={f}>
                                    {f}
                                </option>
                            ))}
                        </optgroup>
                    ))}
            </SelectSetting>

            <RangeSetting
                label="Code Font Size"
                min={8}
                max={160}
                step={1}
                value={config.performanceFontSize ?? d.fontSize}
                format={(value) => `${value}px`}
                onChange={(performanceFontSize) =>
                    onConfigChange({ performanceFontSize })
                }
            />
            <RangeSetting
                label="Code Line Height"
                min={1}
                max={3}
                step={0.05}
                value={config.performanceLineHeight ?? d.lineHeight}
                format={fixed(2)}
                onChange={(performanceLineHeight) =>
                    onConfigChange({ performanceLineHeight })
                }
            />
            <ToggleSetting
                label="Code Ligatures"
                checked={config.performanceFontLigatures ?? d.ligatures}
                onChange={(performanceFontLigatures) =>
                    onConfigChange({ performanceFontLigatures })
                }
            />
            <ToggleSetting
                label="Code Line Numbers"
                checked={config.performanceLineNumbers ?? d.lineNumbers}
                onChange={(performanceLineNumbers) =>
                    onConfigChange({ performanceLineNumbers })
                }
            />
            <RangeSetting
                label="Scroll Speed"
                min={0}
                max={1}
                step={0.01}
                value={config.performanceScrollSpeed ?? d.scrollSpeed}
                format={(value) =>
                    value >= 1
                        ? 'Instant'
                        : `${Math.round(scrollEaseMs(value))} ms`
                }
                onChange={(performanceScrollSpeed) =>
                    onConfigChange({ performanceScrollSpeed })
                }
            />
        </>
    );
}
