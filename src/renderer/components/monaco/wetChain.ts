/**
 * A Prettier plugin that keeps a `.wet(...)` call on the line of the chained
 * call it mixes against, so a broken chain reads one effect per line:
 *
 *     $saw('c3')
 *       .lpf('800hz').wet(2)
 *       .plate().wet(1)
 *
 * The `wetOnSameLine` option turns it off. It wraps the estree printer: the
 * `wet` property prints under a label, and the head of each chain drops the
 * line break Prettier puts before it. A call that ends in a line comment or a
 * blank line keeps its `.wet` on the next line. It matches Prettier's printed
 * docs by shape, which `wetChain.test.ts` checks across Prettier upgrades.
 */
import type { AstPath, Doc, Plugin, Printer } from 'prettier';
import { builders } from 'prettier/doc';
import * as prettierEstree from 'prettier/plugins/estree';

interface Node {
    type: string;
    [key: string]: unknown;
}

const WET_LABEL = 'dsl-wet-chain';

const estree = (prettierEstree as unknown as { printers: { estree: Printer } })
    .printers.estree;

function isWetCallee(node: unknown): boolean {
    const member = node as Node | undefined;
    return (
        member?.type === 'MemberExpression' &&
        !member.computed &&
        (member.property as Node).type === 'Identifier' &&
        (member.property as { name: string }).name === 'wet'
    );
}

/** Whether `node` is the `wet` in a `.wet(...)` call. */
function isWetProperty(path: AstPath<Node>): boolean {
    const { node, parent, grandparent } = path;
    return (
        node.type === 'Identifier' &&
        parent?.property === node &&
        isWetCallee(parent) &&
        (grandparent?.type === 'CallExpression' ||
            grandparent?.type === 'OptionalCallExpression') &&
        grandparent.callee === parent
    );
}

/** Whether `node` is a call whose callee chain holds a `.wet(...)` call. */
function chainHasWet(node: Node): boolean {
    let current: Node | undefined = node;
    while (current) {
        if (
            (current.type === 'CallExpression' ||
                current.type === 'OptionalCallExpression') &&
            isWetCallee(current.callee)
        ) {
            return true;
        }
        current = (current.callee ?? current.object) as Node | undefined;
    }
    return false;
}

function isHardline(doc: Doc): boolean {
    return (
        Array.isArray(doc) &&
        doc.length === 2 &&
        typeof doc[0] === 'object' &&
        (doc[0] as { type?: string }).type === 'line' &&
        (doc[0] as { hard?: boolean }).hard === true &&
        !(doc[0] as { literal?: boolean }).literal &&
        (doc[1] as { type?: string }).type === 'break-parent'
    );
}

/** Whether `doc` prints `.wet` first. */
function startsWithWet(doc: Doc): boolean {
    const pieces: Doc[] = [];
    collectPieces(doc, pieces, 2);
    return (
        (pieces[0] === '.' || pieces[0] === '?.') &&
        typeof pieces[1] === 'object' &&
        (pieces[1] as { label?: unknown }).label === WET_LABEL
    );
}

/** The first `limit` printed pieces of `doc`, skipping empty strings. */
function collectPieces(doc: Doc, out: Doc[], limit: number): void {
    if (out.length >= limit) {
        return;
    }
    if (typeof doc === 'string') {
        if (doc !== '') {
            out.push(doc);
        }
    } else if (Array.isArray(doc)) {
        for (const part of doc) {
            collectPieces(part, out, limit);
        }
    } else if (doc.type === 'label' && doc.label === WET_LABEL) {
        out.push(doc);
    } else if (
        doc.type === 'label' ||
        doc.type === 'group' ||
        doc.type === 'indent' ||
        doc.type === 'align'
    ) {
        collectPieces(doc.contents, out, limit);
    } else {
        out.push(doc);
    }
}

/** Whether `doc` ends in a line comment or a line break of its own. */
function endsInBreak(doc: Doc): boolean {
    if (typeof doc === 'string') {
        return false;
    }
    if (Array.isArray(doc)) {
        for (let i = doc.length - 1; i >= 0; i--) {
            const part = doc[i];
            if (
                part === '' ||
                (part as { type?: string }).type === 'break-parent'
            ) {
                continue;
            }
            return endsInBreak(part);
        }
        return false;
    }
    if (doc.type === 'line-suffix' || (doc.type === 'line' && doc.hard)) {
        return true;
    }
    return 'contents' in doc && endsInBreak(doc.contents);
}

/**
 * `parts` with the hard line before each `.wet` dropped. `before` is the
 * doc printed just ahead of `parts`, which a leading hard line follows.
 */
function joinParts(parts: Doc[], before: Doc): Doc[] {
    const out: Doc[] = [];
    let previous = before;
    for (let i = 0; i < parts.length; i++) {
        const piece = join(parts[i], previous);
        const next = parts[i + 1];
        if (
            isHardline(piece) &&
            next !== undefined &&
            startsWithWet(next) &&
            !endsInBreak(previous)
        ) {
            continue;
        }
        out.push(piece);
        if (piece !== '') {
            previous = piece;
        }
    }
    return out;
}

/** Docs `join` returned, which a chain enclosing them need not walk again. */
const joinedDocs = new WeakSet<object>();

function join(doc: Doc, before: Doc = ''): Doc {
    if (typeof doc === 'string' || joinedDocs.has(doc)) {
        return doc;
    }
    if (Array.isArray(doc)) {
        return joinParts(doc, before);
    }
    switch (doc.type) {
        case 'group': {
            // A conditional group prints its first expanded state as its
            // contents, so the two stay one doc.
            const expandedStates = doc.expandedStates?.map((state) =>
                join(state, before),
            );
            return {
                ...doc,
                contents: expandedStates?.[0] ?? join(doc.contents, before),
                expandedStates,
            };
        }
        case 'fill':
            return { ...doc, parts: joinParts(doc.parts, before) };
        case 'if-break':
            return {
                ...doc,
                breakContents: join(doc.breakContents, before),
                flatContents: join(doc.flatContents, before),
            };
        case 'align':
        case 'indent':
        case 'indent-if-break':
        case 'label':
        case 'line-suffix':
            return {
                ...doc,
                contents: join((doc as { contents: Doc }).contents, before),
            } as Doc;
        default:
            return doc;
    }
}

function joinWetCalls(doc: Doc): Doc {
    const joined = join(doc);
    if (typeof joined === 'object') {
        joinedDocs.add(joined);
    }
    return joined;
}

export const wetChainPlugin: Plugin = {
    options: {
        wetOnSameLine: {
            category: 'JavaScript',
            default: true,
            description: 'Keep .wet() on the line of the effect it mixes',
            type: 'boolean',
        },
    },
    printers: {
        estree: {
            ...estree,
            print(path, options, print, args) {
                const printed = estree.print(path, options, print, args);
                if (options.wetOnSameLine === false) {
                    return printed;
                }
                const node = path.node as Node;
                if (isWetProperty(path as AstPath<Node>)) {
                    return builders.label(WET_LABEL, printed);
                }
                return (node.type === 'CallExpression' ||
                    node.type === 'OptionalCallExpression') &&
                    chainHasWet(node)
                    ? joinWetCalls(printed)
                    : printed;
            },
        },
    },
};
