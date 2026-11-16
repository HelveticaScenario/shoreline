/**
 * `$'c e g'` and `$"c e g"` read like a mini-notation pattern but are not
 * JavaScript: `$` takes a template literal (`` $`c e g` ``) or a call
 * (`$('c e g')`). The parser's own errors there (`',' expected.`) don't say
 * so, so the editor shows this diagnostic in their place.
 */
import type * as TS from 'typescript';

/** The diagnostic's code; outside TypeScript's own range. */
export const DOLLAR_QUOTE_CODE = 90001;

export interface DollarQuote {
    /** Offset of the `$`. */
    start: number;
    /** Offset just past the closing quote. */
    end: number;
    /** Offset just past the calls and member reads chained on the string. */
    chainEnd: number;
    /** The string's text between its quotes, as written. */
    body: string;
}

/** Whether `node` is `.x`, `[i]` or `(…)` applied to `receiver`. */
function isChainedOn(ts: typeof TS, node: TS.Node, receiver: TS.Node): boolean {
    return (
        (ts.isPropertyAccessExpression(node) ||
            ts.isElementAccessExpression(node) ||
            ts.isCallExpression(node)) &&
        node.expression === receiver
    );
}

/**
 * The whole chain hung off `node`: for the string in `'c e g'.out()`, the
 * call `'c e g'.out()`.
 */
function chainOf(ts: typeof TS, node: TS.Node): TS.Node {
    let chain = node;
    while (isChainedOn(ts, chain.parent, chain)) {
        chain = chain.parent;
    }
    return chain;
}

/** Every `$` written directly before a closed quoted string in `sourceFile`. */
export function findDollarQuotes(
    ts: typeof TS,
    sourceFile: TS.SourceFile,
): DollarQuote[] {
    const text = sourceFile.text;
    if (!text.includes("$'") && !text.includes('$"')) {
        return [];
    }
    const dollarEnds = new Map<number, number>();
    const strings: TS.StringLiteral[] = [];
    const visit = (node: TS.Node): void => {
        if (ts.isIdentifier(node) && node.text === '$') {
            dollarEnds.set(node.end, node.getStart(sourceFile));
        } else if (ts.isStringLiteral(node) && !node.isUnterminated) {
            strings.push(node);
        }
        ts.forEachChild(node, visit);
    };
    visit(sourceFile);
    const found: DollarQuote[] = [];
    for (const literal of strings) {
        const literalStart = literal.getStart(sourceFile);
        const start = dollarEnds.get(literalStart);
        if (start === undefined) {
            continue;
        }
        found.push({
            start,
            end: literal.end,
            chainEnd: chainOf(ts, literal).end,
            body: text.slice(literalStart + 1, literal.end - 1),
        });
    }
    return found;
}

/** `body`, a quoted string's text as written, as the text of a template literal. */
export function templateBody(body: string): string {
    return body.replace(/\\[\s\S]|`|\$\{/g, (match) => {
        if (match === "\\'" || match === '\\"') {
            return match[1];
        }
        return match === '`' || match === '${' ? `\\${match}` : match;
    });
}

/** `written` as a pattern in backticks, or null when it is not a `$'…'`. */
export function backtickedPattern(written: string): string | null {
    const quoted = /^\$(['"])([\s\S]*)\1$/.exec(written);
    return quoted ? `$\`${templateBody(quoted[2])}\`` : null;
}

/**
 * Type errors with the `$'…'`s they follow from, those on the string and on
 * what is chained on it (`.out()` on a string), dropped.
 */
export function withoutDollarQuoteTypeErrors(
    ts: typeof TS,
    sourceFile: TS.SourceFile,
    diagnostics: readonly TS.Diagnostic[],
): TS.Diagnostic[] {
    const quotes = findDollarQuotes(ts, sourceFile);
    return diagnostics.filter(
        (d) =>
            d.start === undefined ||
            !quotes.some((q) => d.start! >= q.start && d.start! < q.chainEnd),
    );
}

/**
 * Parser errors with each one inside a `$'…'` replaced by one diagnostic
 * over the whole of it, saying to use backticks.
 */
export function withDollarQuoteDiagnostics(
    ts: typeof TS,
    sourceFile: TS.SourceFile,
    diagnostics: readonly TS.Diagnostic[],
): TS.Diagnostic[] {
    const quotes = findDollarQuotes(ts, sourceFile);
    if (quotes.length === 0) {
        return [...diagnostics];
    }
    const kept = diagnostics.filter(
        (d) =>
            d.start === undefined ||
            !quotes.some((q) => d.start! >= q.start && d.start! < q.end),
    );
    const added = quotes.map(
        (q): TS.Diagnostic => ({
            category: ts.DiagnosticCategory.Error,
            code: DOLLAR_QUOTE_CODE,
            file: sourceFile,
            length: q.end - q.start,
            messageText: `Patterns use backticks: write $\`${templateBody(q.body)}\`, or call $(${sourceFile.text.slice(q.start + 1, q.end)}).`,
            start: q.start,
        }),
    );
    return [...kept, ...added].sort((a, b) => (a.start ?? 0) - (b.start ?? 0));
}
