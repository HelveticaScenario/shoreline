import { useQuery } from '@tanstack/react-query';
import { useEffect } from 'react';
import type { AppConfig, PrettierConfig } from '../../shared/ipcTypes';
import {
    DEFAULT_CURSOR_STYLE,
    type CursorStyle,
} from '../../shared/cursorStyle';
import {
    performanceViewSettings,
    type PerformanceViewSettings,
} from '../performance/settings';
import { configQuery } from '../queries/config';
import { bundledThemes } from './bundled';
import type { AppTheme } from './types';
import { mapVSCodeTheme } from './types';

interface ThemeSettings {
    theme: AppTheme;
    themes: AppTheme[];
    cursorStyle: CursorStyle;
    font: string;
    fontLigatures: boolean;
    fontSize: number;
    prettierConfig: PrettierConfig;
    codeBackdropOpacity: number;
    performanceScale: number;
    performanceView: PerformanceViewSettings;
}

const themeMap = new Map<string, AppTheme>();
for (const t of bundledThemes) {
    const mapped = mapVSCodeTheme(t);
    themeMap.set(mapped.id, mapped);
}
const themes = Array.from(themeMap.values());
const FALLBACK_THEME = mapVSCodeTheme(bundledThemes[0]);
const NO_PRETTIER_CONFIG: PrettierConfig = {};

function themeSettings(config: AppConfig): ThemeSettings {
    return {
        codeBackdropOpacity: config.codeBackdropOpacity ?? 0.6,
        cursorStyle: config.cursorStyle ?? DEFAULT_CURSOR_STYLE,
        font: config.font ?? 'Fira Code',
        fontLigatures: config.fontLigatures ?? false,
        fontSize: config.fontSize ?? 17,
        performanceScale: config.performanceScale ?? 1,
        performanceView: performanceViewSettings(config),
        prettierConfig: config.prettier ?? NO_PRETTIER_CONFIG,
        theme: themeMap.get(config.theme ?? 'modular-dark') ?? FALLBACK_THEME,
        themes,
    };
}

const DEFAULT_SETTINGS = themeSettings({});

/** The look the config file sets, with defaults for what it leaves out. */
export function useTheme(): ThemeSettings {
    const { data } = useQuery({ ...configQuery, select: themeSettings });
    return data ?? DEFAULT_SETTINGS;
}

/** Keeps the document's colour variables on the configured theme. */
export function useThemeCss(): void {
    const { theme } = useTheme();
    useEffect(() => {
        const root = document.documentElement;
        const c = theme.colors;
        const vars: Record<string, string> = {
            '--accent-muted': c.accentMuted,
            '--accent-primary': c.accentPrimary,
            '--accent-secondary': c.accentSecondary,
            '--bg-active': c.bgActive,
            '--bg-hover': c.bgHover,
            '--bg-primary': c.bgPrimary,
            '--bg-secondary': c.bgSecondary,
            '--bg-tertiary': c.bgTertiary,
            '--border-default': c.borderDefault,
            '--border-subtle': c.borderSubtle,
            '--color-error': c.colorError,
            '--color-info': c.colorInfo,
            '--color-success': c.colorSuccess,
            '--color-warning': c.colorWarning,
            '--text-bright': c.textBright,
            '--text-muted': c.textMuted,
            '--text-primary': c.textPrimary,
            '--text-secondary': c.textSecondary,
        };
        for (const [name, value] of Object.entries(vars)) {
            root.style.setProperty(name, value);
        }
        root.style.colorScheme = theme.type;
    }, [theme]);
}
