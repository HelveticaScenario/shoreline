import { resolvePrettierOptions } from '../components/monaco/formattingProvider';
import { requote } from '../dsl/requote';
import { useTheme } from '../themes/ThemeContext';

/** Quotes DSL code shown to the user, such as an example, in the quote
 *  style Format Document uses. */
export function useQuoteExample(): (code: string) => string {
    const { prettierConfig } = useTheme();
    const quotes = resolvePrettierOptions(prettierConfig).quotes;
    return (code: string) => requote(code, quotes);
}
