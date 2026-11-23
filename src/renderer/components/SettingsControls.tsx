import type { ReactNode } from 'react';

/** A titled dropdown; `children` are its options. */
export function SelectSetting({
    label,
    value,
    disabled,
    onChange,
    onApproach,
    note,
    children,
}: {
    label: string;
    value: string | number;
    disabled?: boolean;
    onChange: (value: string) => void;
    /**
     * Runs as the pointer or focus reaches the dropdown, ahead of the click
     * or key that opens it.
     */
    onApproach?: () => void;
    /** Shown under the dropdown. */
    note?: ReactNode;
    children: ReactNode;
}) {
    return (
        <div className="settings-section">
            <h3>{label}</h3>
            <select
                className="device-select"
                onPointerEnter={onApproach}
                onFocus={onApproach}
                value={value}
                onChange={(e) => onChange(e.target.value)}
                disabled={disabled}
            >
                {children}
            </select>
            {note}
        </div>
    );
}

export function RangeSetting({
    label,
    min,
    max,
    step,
    value,
    format,
    onChange,
}: {
    label: string;
    min: number;
    max: number;
    step: number;
    value: number;
    format: (value: number) => string;
    onChange: (value: number) => void;
}) {
    return (
        <div className="settings-section">
            <h3>{label}</h3>
            <div className="settings-row">
                <input
                    type="range"
                    className="settings-range"
                    min={min}
                    max={max}
                    step={step}
                    value={value}
                    onChange={(e) => onChange(Number(e.target.value))}
                />
                <span className="settings-range-value">{format(value)}</span>
            </div>
        </div>
    );
}

export function ToggleSetting({
    label,
    checked,
    onChange,
}: {
    label: string;
    checked: boolean;
    onChange: (checked: boolean) => void;
}) {
    return (
        <div className="settings-section">
            <h3>{label}</h3>
            <label className="settings-toggle-label">
                <input
                    type="checkbox"
                    className="settings-toggle"
                    checked={checked}
                    onChange={(e) => onChange(e.target.checked)}
                />
            </label>
        </div>
    );
}
