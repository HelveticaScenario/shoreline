import type {
    AppConfig,
    PrettierConfig,
    QuoteStyle,
} from '../../shared/ipcTypes';
import { resolvePrettierOptions } from './monaco/formattingProvider';
import { SelectSetting } from './SettingsControls';

const TRAILING_COMMA_OPTIONS = ['all', 'es5', 'none'] as const;

const QUOTE_OPTIONS: { label: string; value: QuoteStyle }[] = [
    { label: "Single: 'text'", value: 'single' },
    { label: 'Double: "text"', value: 'double' },
    { label: 'Backtick: `text`', value: 'backtick' },
];

interface FormatterSettingsTabProps {
    config: AppConfig;
    onConfigChange: (partial: Partial<AppConfig>) => void;
}

export function FormatterSettingsTab({
    config,
    onConfigChange,
}: FormatterSettingsTabProps) {
    const prettier = config.prettier ?? {};

    const updatePrettier = (patch: Partial<PrettierConfig>) => {
        onConfigChange({ prettier: { ...prettier, ...patch } });
    };

    return (
        <div className="settings-tab-content">
            <p className="settings-description">
                Configure Prettier formatting options for the patch editor.
                These are merged with built-in defaults.
            </p>

            <div className="settings-section">
                <h3>Print Width</h3>
                <div className="settings-row">
                    <input
                        type="number"
                        className="settings-number-input"
                        min={20}
                        max={200}
                        value={prettier.printWidth ?? 60}
                        onChange={(e) =>
                            updatePrettier({
                                printWidth: Number(e.target.value),
                            })
                        }
                    />
                    <span className="settings-hint">characters per line</span>
                </div>
            </div>

            <div className="settings-section">
                <h3>Tab Width</h3>
                <div className="settings-row">
                    <input
                        type="number"
                        className="settings-number-input"
                        min={1}
                        max={8}
                        value={prettier.tabWidth ?? 2}
                        onChange={(e) =>
                            updatePrettier({ tabWidth: Number(e.target.value) })
                        }
                    />
                    <span className="settings-hint">spaces per indent</span>
                </div>
            </div>

            <div className="settings-section">
                <h3>Semicolons</h3>
                <label className="settings-toggle">
                    <input
                        type="checkbox"
                        checked={prettier.semi ?? false}
                        onChange={(e) =>
                            updatePrettier({ semi: e.target.checked })
                        }
                    />
                    <span>Add semicolons at the end of statements</span>
                </label>
            </div>

            <SelectSetting
                label="Quotes"
                value={resolvePrettierOptions(prettier).quotes}
                onChange={(value) =>
                    updatePrettier({
                        quotes: value as QuoteStyle,
                        singleQuote: undefined,
                    })
                }
                note={
                    <span className="settings-hint">
                        Backtick keeps quotes where JavaScript requires them,
                        such as import paths and object keys
                    </span>
                }
            >
                {QUOTE_OPTIONS.map(({ label, value }) => (
                    <option key={value} value={value}>
                        {label}
                    </option>
                ))}
            </SelectSetting>

            <SelectSetting
                label="Trailing Commas"
                value={(prettier.trailingComma as string) ?? 'all'}
                onChange={(value) =>
                    updatePrettier({
                        trailingComma: value as 'all' | 'es5' | 'none',
                    })
                }
            >
                {TRAILING_COMMA_OPTIONS.map((opt) => (
                    <option key={opt} value={opt}>
                        {opt}
                    </option>
                ))}
            </SelectSetting>
        </div>
    );
}
