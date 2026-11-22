/**
 * Quotes the strings in DSL source the way Format Document would for a
 * quote style, leaving everything else as written. Examples keep their
 * hand-made layout (aligned comments, long lines) while their strings
 * follow the user's setting.
 */
import ts from 'typescript';

import { parseTransient } from '../../shared/dsl/ast/parse';
import type { QuoteStyle } from '../../shared/ipcTypes';
import { templateBody } from '../components/monaco/quoteStyle';

/** Whether the grammar requires a plain string literal at `node`. */
function requiresQuotedString(node: ts.Node): boolean {
    const parent = node.parent;
    if (
        (ts.isPropertyAssignment(parent) ||
            ts.isMethodDeclaration(parent) ||
            ts.isPropertyDeclaration(parent) ||
            ts.isGetAccessorDeclaration(parent) ||
            ts.isSetAccessorDeclaration(parent)) &&
        parent.name === node
    ) {
        return true;
    }
    return (
        ts.isImportDeclaration(parent) ||
        ts.isExportDeclaration(parent) ||
        ts.isExternalModuleReference(parent) ||
        ts.isImportSpecifier(parent) ||
        ts.isExportSpecifier(parent) ||
        ts.isNamespaceExport(parent) ||
        ts.isImportAttribute(parent) ||
        ts.isJsxAttribute(parent) ||
        (ts.isCallExpression(parent) &&
            parent.expression.kind === ts.SyntaxKind.ImportKeyword)
    );
}

type Quote = "'" | '"';

/** Prettier's choice: the preferred quote, unless the body holds more of
 *  it than of the other. */
function quoteFor(body: string, preferred: Quote): Quote {
    const other = preferred === "'" ? '"' : "'";
    const count = (q: string) => body.split(q).length - 1;
    return count(preferred) > count(other) ? other : preferred;
}

/**
 * A string's body re-quoted in `quote`, as Prettier prints it: a body
 * already in `quote` stays as written; otherwise only quote escapes change.
 */
function quoted(body: string, from: string | null, quote: Quote): string {
    if (from === quote) {
        return `${quote}${body}${quote}`;
    }
    const other = quote === "'" ? '"' : "'";
    const inner = body.replace(
        /\\([\s\S])|(["'])/g,
        (escape, escaped: string | undefined, bare: string | undefined) => {
            if (escaped === other) {
                return escaped;
            }
            return bare === quote ? `\\${bare}` : escape;
        },
    );
    return `${quote}${inner}${quote}`;
}

/** Whether `statement` is in the directive prologue of a script or
 *  function body, where Prettier keeps a quote-holding string as written. */
function isDirective(statement: ts.ExpressionStatement): boolean {
    const parent = statement.parent;
    if (
        !ts.isSourceFile(parent) &&
        !(ts.isBlock(parent) && ts.isFunctionLike(parent.parent))
    ) {
        return false;
    }
    for (const sibling of parent.statements) {
        if (sibling === statement) {
            return true;
        }
        if (
            !ts.isExpressionStatement(sibling) ||
            !ts.isStringLiteral(sibling.expression)
        ) {
            return false;
        }
    }
    return false;
}

function rewriteString(
    node: ts.StringLiteral,
    raw: string,
    quotes: QuoteStyle,
): string | null {
    const body = raw.slice(1, -1);
    const preferred = quotes === 'double' ? '"' : "'";
    const statement = ts.isExpressionStatement(node.parent);
    if (statement && isDirective(node.parent)) {
        return /["']/.test(body) ? null : `${preferred}${body}${preferred}`;
    }
    // A template standing alone as a statement would tag the line before it.
    if (quotes === 'backtick' && !statement && !requiresQuotedString(node)) {
        const template = templateBody(body);
        if (template !== null) {
            return `\`${template}\``;
        }
    }
    return quoted(body, raw[0], quoteFor(body, preferred));
}

/** Whether a template body holds an octal-like escape, which a quoted
 *  string would read differently. */
function hasOctalEscape(body: string): boolean {
    for (let i = 0; i < body.length; i++) {
        if (body[i] === '\\') {
            const next = body[i + 1];
            if (
                /[1-9]/.test(next) ||
                (next === '0' && /\d/.test(body[i + 2] ?? ''))
            ) {
                return true;
            }
            i++;
        }
    }
    return false;
}

/**
 * The quoted string a template literal becomes outside backtick mode: one
 * with no interpolation, tag or line break, that does not stand alone as a
 * statement.
 */
function rewriteTemplate(
    node: ts.NoSubstitutionTemplateLiteral,
    raw: string,
    quotes: QuoteStyle,
): string | null {
    if (
        quotes === 'backtick' ||
        ts.isTaggedTemplateExpression(node.parent) ||
        ts.isExpressionStatement(node.parent)
    ) {
        return null;
    }
    const body = raw.slice(1, -1);
    if (/[\n\r\u2028\u2029]/.test(body) || hasOctalEscape(body)) {
        return null;
    }
    const plain = body.replace(/\\([\s\S])/g, (escape, ch: string) =>
        ch === '`' || ch === '$' ? ch : escape,
    );
    const preferred = quotes === 'double' ? '"' : "'";
    return quoted(plain, null, quoteFor(plain, preferred));
}

/**
 * `code` with every string quoted for `quotes`; nothing else changes. A
 * literal left unterminated is kept as written.
 */
export function requote(code: string, quotes: QuoteStyle): string {
    const sourceFile = parseTransient(code);
    const edits: { start: number; end: number; text: string }[] = [];
    const visit = (node: ts.Node): void => {
        if (
            (ts.isStringLiteral(node) ||
                ts.isNoSubstitutionTemplateLiteral(node)) &&
            !node.isUnterminated
        ) {
            const start = node.getStart(sourceFile);
            const raw = code.slice(start, node.end);
            const text = ts.isStringLiteral(node)
                ? rewriteString(node, raw, quotes)
                : rewriteTemplate(node, raw, quotes);
            if (text !== null) {
                edits.push({ end: node.end, start, text });
            }
        }
        ts.forEachChild(node, visit);
    };
    visit(sourceFile);
    let out = code;
    for (const edit of edits.reverse()) {
        out = out.slice(0, edit.start) + edit.text + out.slice(edit.end);
    }
    return out;
}

/** `markdown` with the strings in its JavaScript code blocks quoted for
 *  `quotes`. */
export function requoteMarkdown(markdown: string, quotes: QuoteStyle): string {
    return markdown.replace(
        /^(```(?:js|javascript)\b[^\n]*\n)([\s\S]*?)^```/gm,
        (_match, open: string, code: string) =>
            `${open}${requote(code, quotes)}\`\`\``,
    );
}
