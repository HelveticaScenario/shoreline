import { useEffect, useEffectEvent } from 'react';
import type { Monaco } from '../../hooks/useCustomMonaco';
import type { PrettierConfig } from '../../../shared/ipcTypes';
import type { Schemas } from '../../../shared/dsl/schemaTypeResolver';
import electronAPI from '../../electronAPI';
import { apiIndexFor } from '../../dsl/apiSignatures';
import { setupMonacoJavascript } from '../monaco/monacoLanguage';
import {
    registerDslFormattingProvider,
    resolvePrettierOptions,
} from '../monaco/formattingProvider';
import { registerMidiCompletionProvider } from '../monaco/midiCompletionProvider';
import { registerWorkspacePathCompletionProvider } from '../monaco/workspacePathCompletion';
import { registerControlQuickFixProvider } from '../monaco/controlQuickFixProvider';
import { registerRefactorProvider } from '../monaco/refactorProvider';
import { registerKeybindingsCompletionProvider } from '../monaco/keybindingsCompletion';
import { registerKeybindingsDiagnostics } from '../monaco/keybindingsDiagnostics';

interface DslLanguageServicesParams {
    monaco: Monaco | null;
    /** The DSL's TypeScript declarations; null until they have loaded. */
    libSource: string | null;
    schemas: Schemas;
    prettierConfig: PrettierConfig;
}

/**
 * Registers the language features every model shares: the DSL's types and
 * providers, formatting, completions, quick fixes and refactors. Providers
 * register in a fixed order, which Monaco uses to rank them.
 */
export function useDslLanguageServices({
    monaco,
    libSource,
    schemas,
    prettierConfig,
}: DslLanguageServicesParams) {
    const currentQuotes = useEffectEvent(
        () => resolvePrettierOptions(prettierConfig).quotes,
    );

    useEffect(() => {
        if (!monaco || !libSource) {
            return;
        }
        return setupMonacoJavascript(monaco, libSource, {
            quotes: () => currentQuotes(),
            schemas,
        });
    }, [monaco, libSource, schemas]);

    useEffect(() => {
        if (!monaco) {
            return;
        }
        const disposable = registerDslFormattingProvider(
            monaco,
            prettierConfig,
        );
        return () => disposable.dispose();
    }, [monaco, prettierConfig]);

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
        const formatOptions = resolvePrettierOptions(prettierConfig);
        const provider = registerControlQuickFixProvider(
            monaco,
            () => ({ index: apiIndexFor(libSource), schemas }),
            () => formatOptions,
        );
        return () => provider.dispose();
    }, [monaco, schemas, libSource, prettierConfig]);

    useEffect(() => {
        if (!monaco) {
            return;
        }
        const formatOptions = resolvePrettierOptions(prettierConfig);
        const provider = registerRefactorProvider(monaco, () => formatOptions);
        return () => provider.dispose();
    }, [monaco, prettierConfig]);

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
