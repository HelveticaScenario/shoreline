import { useEffect, useEffectEvent } from 'react';
import type { Monaco } from '../../hooks/useCustomMonaco';
import type { Schemas } from '../../../shared/dsl/schemaTypeResolver';
import type { AppTheme } from '../../themes/types';
import { configSchema } from '../../configSchema';
import electronAPI from '../../electronAPI';
import { apiIndexFor } from '../../dsl/apiSignatures';
import { applyMonacoTheme } from '../monaco/theme';
import { registerConfigSchema } from '../monaco/jsonSchema';
import { setupMonacoJavascript } from '../monaco/monacoLanguage';
import { registerDslFormattingProvider } from '../monaco/formattingProvider';
import { registerMidiCompletionProvider } from '../monaco/midiCompletionProvider';
import { registerWorkspacePathCompletionProvider } from '../monaco/workspacePathCompletion';
import { registerControlQuickFixProvider } from '../monaco/controlQuickFixProvider';
import { registerRefactorProvider } from '../monaco/refactorProvider';
import { registerKeybindingsCompletionProvider } from '../monaco/keybindingsCompletion';
import { registerKeybindingsDiagnostics } from '../monaco/keybindingsDiagnostics';
import type { EditorCodeFormat } from './codeFormat';

interface MonacoRegistrationsParams {
    monaco: Monaco | null;
    theme: AppTheme;
    /** The id `theme` is defined under, which the Editor selects. */
    themeId: string;
    /** The DSL's TypeScript declarations; null until they have loaded. */
    libSource: string | null;
    schemas: Schemas;
    codeFormat: EditorCodeFormat;
}

/**
 * Owns everything registered on the shared Monaco instance, which every
 * model and editor sees: the theme, the config file's JSON schema, the DSL's
 * types, and the formatting, completion, quick-fix, refactor and diagnostics
 * providers. Whatever attaches to the one editor instance lives in the
 * `useEditor*` hooks.
 */
export function useMonacoRegistrations({
    monaco,
    theme,
    themeId,
    libSource,
    schemas,
    codeFormat,
}: MonacoRegistrationsParams) {
    const currentFormat = useEffectEvent(() => codeFormat);

    useEffect(() => {
        if (!monaco) {
            return;
        }
        applyMonacoTheme(monaco, theme, themeId);
    }, [monaco, theme, themeId]);

    useEffect(() => {
        if (!monaco) {
            return;
        }
        registerConfigSchema(monaco, configSchema);
    }, [monaco]);

    useEffect(() => {
        if (!monaco || !libSource) {
            return;
        }
        return setupMonacoJavascript(monaco, libSource, {
            quotes: () => currentFormat().options.quotes,
            schemas,
        });
    }, [monaco, libSource, schemas]);

    useEffect(() => {
        if (!monaco) {
            return;
        }
        const disposable = registerDslFormattingProvider(
            monaco,
            () => currentFormat().options,
        );
        return () => disposable.dispose();
    }, [monaco]);

    useEffect(() => {
        if (!monaco) {
            return;
        }
        const provider = registerMidiCompletionProvider(monaco, () =>
            electronAPI.midi.listInputs(),
        );
        return () => provider.dispose();
    }, [monaco]);

    useEffect(() => {
        if (!monaco) {
            return;
        }
        const provider = registerWorkspacePathCompletionProvider(monaco, () =>
            electronAPI.filesystem.listFiles(),
        );
        return () => provider.dispose();
    }, [monaco]);

    // Quick fix: turn an API param into a control call chosen from its
    // metadata.
    useEffect(() => {
        if (!monaco || schemas.length === 0) {
            return;
        }
        const provider = registerControlQuickFixProvider(
            monaco,
            () => ({ index: apiIndexFor(libSource), schemas }),
            () => currentFormat().options,
        );
        return () => provider.dispose();
    }, [monaco, schemas, libSource]);

    useEffect(() => {
        if (!monaco) {
            return;
        }
        const provider = registerRefactorProvider(
            monaco,
            () => currentFormat().options,
        );
        return () => provider.dispose();
    }, [monaco]);

    // Autocomplete command ids and `when` context keys in the keybindings.json
    // buffer, and flag unsupported `when` operators as warnings. Both scope
    // themselves to that model.
    useEffect(() => {
        if (!monaco) {
            return;
        }
        const provider = registerKeybindingsCompletionProvider(monaco);
        const diagnostics = registerKeybindingsDiagnostics(monaco);
        return () => {
            provider.dispose();
            diagnostics.dispose();
        };
    }, [monaco]);
}
