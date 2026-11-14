import * as prettier from 'prettier/standalone';
import * as prettierBabel from 'prettier/plugins/babel';
import * as prettierEstree from 'prettier/plugins/estree';
import type { Monaco } from '../../hooks/useCustomMonaco';
import type { PrettierConfig, QuoteStyle } from '../../../shared/ipcTypes';
import { quoteStylePlugin } from './quoteStyle';

export const DEFAULT_PRETTIER_OPTIONS = {
    printWidth: 60,
    quotes: 'single' as QuoteStyle,
    semi: false,
    tabWidth: 2,
    trailingComma: 'all' as const,
};

/** The quote a config asks for. `quotes` decides it; a config without one
 *  may still carry Prettier's own `singleQuote`. */
function quoteStyleOf(config: PrettierConfig): QuoteStyle {
    if (config.quotes) {
        return config.quotes;
    }
    return config.singleQuote === false
        ? 'double'
        : DEFAULT_PRETTIER_OPTIONS.quotes;
}

/** The editor's effective Prettier options: the user's over the defaults,
 *  with Prettier's `singleQuote` derived from `quotes`. Backtick strings
 *  keep single quotes where the syntax requires a quoted string. */
export function resolvePrettierOptions(userConfig: PrettierConfig = {}) {
    const quotes = quoteStyleOf(userConfig);
    return {
        ...DEFAULT_PRETTIER_OPTIONS,
        ...userConfig,
        quotes,
        singleQuote: quotes !== 'double',
    };
}

export type ResolvedPrettierOptions = ReturnType<typeof resolvePrettierOptions>;

/** Format DSL source the way Format Document does. */
export async function formatDsl(
    code: string,
    userConfig: PrettierConfig = {},
): Promise<string> {
    const formatted = await prettier.format(code, {
        ...resolvePrettierOptions(userConfig),
        // Parser and plugins must not be overridden
        parser: 'babel',
        plugins: [prettierBabel, prettierEstree as never, quoteStylePlugin],
    });
    return formatted.trimEnd();
}

export function registerDslFormattingProvider(
    monaco: Monaco,
    userConfig: PrettierConfig = {},
) {
    return monaco.languages.registerDocumentFormattingEditProvider(
        'javascript',
        {
            async provideDocumentFormattingEdits(model) {
                return [
                    {
                        range: model.getFullModelRange(),
                        text: await formatDsl(model.getValue(), userConfig),
                    },
                ];
            },
        },
    );
}
