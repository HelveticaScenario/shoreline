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

/** Prettier's choice: the preferred quote, unless the body holds more of
 *  it than of the other. */
function quoteFor(body: string, preferred: "'" | '"'): "'" | '"' {
    const other = preferred === "'" ? '"' : "'";
    const count = (q: string) => body.split(q).length - 1;
    return count(preferred) > count(other) ? other : preferred;
}

/** A quoted string's body re-escaped for `quote`, as Prettier prints it. */
function quoted(body: string, quote: "'" | '"'): string {
    const other = quote === "'" ? '"' : "'";
    const inner = body.replace(
        /\\([\s\S])|(["'])/g,
        (_match, escaped: string | undefined, bare: string | undefined) => {
            if (bare !== undefined) {
                return bare === quote ? `\\${bare}` : bare;
            }
            if (escaped === other) {
                return escaped;
            }
            return /^[^\n\r"'0-7\\bfnrt-vx\u2028\u2029]$/.test(escaped!)
                ? escaped!
                : `\\${escaped}`;
        },
    );
    return `${quote}${inner}${quote}`;
}

function rewriteString(
    node: ts.StringLiteral,
    raw: string,
    quotes: QuoteStyle,
): string | null {
    const body = raw.slice(1, -1);
    if (quotes === 'backtick' && !requiresQuotedString(node)) {
        const template = templateBody(body);
        if (template !== null) {
            return `\`${template}\``;
        }
    }
    const preferred = quotes === 'double' ? '"' : "'";
    return quoted(body, quoteFor(body, preferred));
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
    if (/[\n\r\u2028\u2029]/.test(body) || /\\[1-9]|\\0\d/.test(body)) {
        return null;
    }
    const plain = body.replace(/\\([\s\S])/g, (escape, ch: string) =>
        ch === '`' || ch === '$' ? ch : escape,
    );
    const preferred = quotes === 'double' ? '"' : "'";
    return quoted(plain, quoteFor(plain, preferred));
}

/** `code` with every string quoted for `quotes`; nothing else changes. */
export function requote(code: string, quotes: QuoteStyle): string {
    const sourceFile = parseTransient(code);
    const edits: { start: number; end: number; text: string }[] = [];
    const visit = (node: ts.Node): void => {
        let text: string | null = null;
        if (
            ts.isStringLiteral(node) &&
            !ts.isExpressionStatement(node.parent)
        ) {
            const start = node.getStart(sourceFile);
            text = rewriteString(node, code.slice(start, node.end), quotes);
        } else if (ts.isNoSubstitutionTemplateLiteral(node)) {
            const start = node.getStart(sourceFile);
            text = rewriteTemplate(node, code.slice(start, node.end), quotes);
        }
        if (text !== null) {
            edits.push({
                end: node.end,
                start: node.getStart(sourceFile),
                text,
            });
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
        /(```(?:js|javascript)\n)([\s\S]*?)(\n```)/g,
        (_match, open: string, code: string, close: string) =>
            open + requote(code, quotes) + close,
    );
}
