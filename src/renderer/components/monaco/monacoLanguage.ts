import type { Monaco } from '../../hooks/useCustomMonaco';
import { applyDslLibToMonaco } from './monacoHelpers';
import { EDITOR_COMPILER_OPTIONS } from './compilerOptions';
import { registerDollarQuoteQuickFix } from './dollarQuoteQuickFix';
import { registerJsDocLinkProviders } from './jsDocLinkProviders';
import { registerRenameProvider } from './renameProvider';
import { registerOutputResolutionHints } from './outputResolutionHints';
import type { ModuleSchema } from '@modular/core';
import type { QuoteStyle } from '../../../shared/ipcTypes';

export interface MonacoSetupOptions {
    /** Module schemas for building symbol sets */
    schemas?: ModuleSchema[];
    /** The quote style code examples in hovers and completions follow. */
    quotes: () => QuoteStyle;
}

/**
 * Bracket and quote pairs matching VS Code's JavaScript language configuration.
 * Monaco's bundled JavaScript configuration declares no `surroundingPairs`, so
 * the editor falls back to `autoClosingPairs` — which omits `<`/`>` and
 * therefore replaces a selection rather than wrapping it. Declaring the pairs
 * restores wrapping for `<>` alongside `()`, `[]`, and `{}`, which
 * mini-notation uses for grouping inside pattern strings.
 *
 * Colorization is left unconfigured: with no `colorizedBracketPairs`, the
 * editor excludes `<`/`>` from bracket-pair colors, so comparison operators
 * keep the operator color. Configuring the field opts them in — which is why
 * VS Code colorizes angle brackets in TypeScript but not in JavaScript.
 */
function configureJavascriptPairs(monaco: Monaco) {
    return monaco.languages.setLanguageConfiguration('javascript', {
        autoCloseBefore: ';:.,=}])>` \n\t',
        autoClosingPairs: [
            { close: '}', open: '${' },
            { close: '}', open: '{' },
            { close: ']', open: '[' },
            { close: ')', open: '(' },
            { close: "'", notIn: ['string', 'comment'], open: "'" },
            { close: '"', notIn: ['string'], open: '"' },
            { close: '`', notIn: ['string', 'comment'], open: '`' },
            { close: ' */', notIn: ['string'], open: '/**' },
        ],
        // Typing `$` then `{` around a selection produces `${selection}`; the
        // `${` entry never matches on its own, since surrounding keys off a
        // single typed character.
        surroundingPairs: [
            { close: '}', open: '${' },
            { close: '', open: '$' },
            { close: '}', open: '{' },
            { close: ']', open: '[' },
            { close: ')', open: '(' },
            { close: "'", open: "'" },
            { close: '"', open: '"' },
            { close: '`', open: '`' },
            { close: '>', open: '<' },
        ],
    });
}

export function setupMonacoJavascript(
    monaco: Monaco,
    libSource: string,
    options: MonacoSetupOptions,
) {
    const ts = monaco.typescript;
    console.log('Monaco TS version:', ts);
    const jsDefaults = ts.javascriptDefaults;

    jsDefaults.setCompilerOptions(EDITOR_COMPILER_OPTIONS);

    jsDefaults.setDiagnosticsOptions({
        noSemanticValidation: false,
        noSyntaxValidation: false,
    });

    jsDefaults.setEagerModelSync(true);

    const languageConfiguration = configureJavascriptPairs(monaco);
    const { extraLib, extraLibModel } = applyDslLibToMonaco(monaco, libSource);
    const jsDocLinks = registerJsDocLinkProviders(
        monaco,
        options.schemas ?? [],
        options.quotes,
    );
    const rename = registerRenameProvider(monaco);
    const outputHints = registerOutputResolutionHints(monaco);
    const dollarQuoteFix = registerDollarQuoteQuickFix(monaco);

    return () => {
        dollarQuoteFix.dispose();
        outputHints.dispose();
        rename.dispose();
        jsDocLinks.dispose();
        languageConfiguration.dispose();
        extraLib?.dispose();
        extraLibModel?.dispose();
    };
}
