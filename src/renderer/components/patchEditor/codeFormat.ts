import type { PrettierConfig } from '../../../shared/ipcTypes';
import { codeStyleFromPrettier } from '../../dsl/listEdits';
import {
    DEFAULT_PRETTIER_OPTIONS,
    type ResolvedPrettierOptions,
    resolvePrettierOptions,
} from '../monaco/formattingProvider';

/**
 * Everything the editor reads from the user's Prettier config. The component
 * derives it once and passes it to its hooks. A Monaco callback reads the
 * latest one through an effect event when it runs, so a config change does
 * not register that callback's provider again; an effect that hands Monaco
 * a value depends on that value.
 */
export interface EditorCodeFormat {
    /** The effective options Format Document and generated code follow. */
    options: ResolvedPrettierOptions;
    /** The quote character an inserted string literal opens with. */
    quote: string;
    tabSize: number;
}

export function editorCodeFormat(config: PrettierConfig): EditorCodeFormat {
    const options = resolvePrettierOptions(config);
    return {
        options,
        quote: codeStyleFromPrettier(options).quote,
        tabSize: options.tabWidth ?? DEFAULT_PRETTIER_OPTIONS.tabWidth,
    };
}
