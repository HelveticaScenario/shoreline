/**
 * A Prettier plugin for the editor's `quotes` option. With `backtick` it
 * prints every string literal that can be a template literal as one;
 * otherwise it prints every template literal without interpolation, tag or
 * line break as a quoted string. It wraps the babel parser
 * and rewrites the tree before printing, so it must be listed after
 * Prettier's own babel plugin to replace its `babel` parser.
 */
import type { Plugin } from 'prettier';
import * as prettierBabel from 'prettier/plugins/babel';

interface Node {
    type: string;
    [key: string]: unknown;
}

const NON_CHILD_KEYS = new Set([
    'comments',
    'extra',
    'innerComments',
    'leadingComments',
    'loc',
    'range',
    'tokens',
    'trailingComments',
]);

const KEYED_TYPES = new Set([
    'ClassAccessorProperty',
    'ClassMethod',
    'ClassPrivateProperty',
    'ClassProperty',
    'ObjectMethod',
    'ObjectProperty',
]);

const MODULE_SOURCE_TYPES = new Set([
    'ExportAllDeclaration',
    'ExportNamedDeclaration',
    'ImportDeclaration',
    'ImportExpression',
]);

const MODULE_NAME_TYPES = new Set([
    'ExportNamespaceSpecifier',
    'ExportSpecifier',
    'ImportAttribute',
    'ImportSpecifier',
]);

function isNode(value: unknown): value is Node {
    return (
        typeof value === 'object' &&
        value !== null &&
        typeof (value as Node).type === 'string'
    );
}

/** Whether the grammar requires a plain string literal at `key` of `parent`. */
function requiresQuotedString(parent: Node, key: string): boolean {
    if (KEYED_TYPES.has(parent.type)) {
        return key === 'key' && !parent.computed;
    }
    if (MODULE_SOURCE_TYPES.has(parent.type)) {
        return key === 'source';
    }
    return MODULE_NAME_TYPES.has(parent.type) || parent.type === 'JSXAttribute';
}

/**
 * The template-literal body for a quoted string's body, keeping its escapes;
 * null when the string holds a legacy octal or `\8`/`\9` escape, which a
 * template literal forbids.
 */
export function templateBody(quotedBody: string): string | null {
    let out = '';
    for (let i = 0; i < quotedBody.length; i++) {
        const ch = quotedBody[i];
        if (ch === '\\') {
            const next = quotedBody[i + 1];
            if (
                /[1-9]/.test(next) ||
                (next === '0' && /\d/.test(quotedBody[i + 2] ?? ''))
            ) {
                return null;
            }
            out += next === "'" || next === '"' ? next : ch + next;
            i++;
        } else if (ch === '`') {
            out += '\\`';
        } else if (ch === '$' && quotedBody[i + 1] === '{') {
            out += '\\$';
        } else {
            out += ch;
        }
    }
    return out;
}

function toTemplateLiteral(node: Node): Node | null {
    const raw = (node.extra as { raw?: string } | undefined)?.raw;
    if (raw === undefined) {
        return null;
    }
    const body = templateBody(raw.slice(1, -1));
    if (body === null) {
        return null;
    }
    const start = node.start as number;
    const end = node.end as number;
    return {
        end,
        expressions: [],
        loc: node.loc,
        quasis: [
            {
                end: end - 1,
                start: start + 1,
                tail: true,
                type: 'TemplateElement',
                value: { cooked: node.value, raw: body },
            },
        ],
        range: [start, end],
        start,
        type: 'TemplateLiteral',
    };
}

/**
 * The quoted string a template literal can become: one with no
 * interpolation, tag or line break, that does not stand alone as a
 * statement, where a quoted string would read as a directive.
 */
function toStringLiteral(node: Node, parent: Node): Node | null {
    const expressions = node.expressions as unknown[];
    if (
        expressions.length > 0 ||
        parent.type === 'TaggedTemplateExpression' ||
        parent.type === 'ExpressionStatement'
    ) {
        return null;
    }
    const [quasi] = node.quasis as {
        value: { cooked: string | null; raw: string };
    }[];
    const { cooked, raw } = quasi.value;
    if (cooked === null || /[\n\r\u2028\u2029]/.test(raw)) {
        return null;
    }
    // Prettier re-quotes the body for the configured quote, escaping the
    // quotes it now encloses.
    const body = raw.replace(/\\([\s\S])/g, (escape, ch: string) =>
        ch === '`' || ch === '$' ? ch : escape,
    );
    return {
        end: node.end,
        extra: { raw: `"${body}"`, rawValue: cooked },
        loc: node.loc,
        range: [node.start, node.end],
        start: node.start,
        type: 'StringLiteral',
        value: cooked,
    };
}

function convert(child: Node, parent: Node, key: string, toBackticks: boolean) {
    if (toBackticks) {
        return child.type === 'StringLiteral' &&
            !requiresQuotedString(parent, key)
            ? toTemplateLiteral(child)
            : null;
    }
    return child.type === 'TemplateLiteral'
        ? toStringLiteral(child, parent)
        : null;
}

function rewrite(node: Node, toBackticks: boolean): void {
    for (const key of Object.keys(node)) {
        if (NON_CHILD_KEYS.has(key)) {
            continue;
        }
        const value = node[key];
        const children = Array.isArray(value) ? value : [value];
        children.forEach((child, index) => {
            if (!isNode(child)) {
                return;
            }
            const replacement = convert(child, node, key, toBackticks);
            if (replacement === null) {
                rewrite(child, toBackticks);
            } else if (Array.isArray(value)) {
                value[index] = replacement;
            } else {
                node[key] = replacement;
            }
        });
    }
}

const babel = prettierBabel.parsers.babel;

export const quoteStylePlugin: Plugin = {
    options: {
        quotes: {
            category: 'JavaScript',
            choices: [
                { description: 'Single quotes.', value: 'single' },
                { description: 'Double quotes.', value: 'double' },
                {
                    description:
                        'Template literals wherever the syntax allows.',
                    value: 'backtick',
                },
            ],
            default: 'single',
            description: 'How strings are quoted.',
            type: 'choice',
        },
    },
    parsers: {
        babel: {
            ...babel,
            async parse(text, options) {
                const ast = await babel.parse(text, options);
                rewrite(ast as Node, options.quotes === 'backtick');
                return ast;
            },
        },
    },
};
