/**
 * A Prettier plugin that keeps a `.wet(...)` call on the line of the chained
 * call it mixes against, so a broken chain reads one effect per line:
 *
 *     $saw('c3')
 *       .lpf('800hz').wet(2)
 *       .plate().wet(1)
 *
 * It wraps the estree printer: the `wet` property prints under a label, and
 * the head of each chain drops the line break Prettier puts before it.
 */
import type { AstPath, Doc, Plugin, Printer } from 'prettier';
import { builders, utils } from 'prettier/doc';
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

function joinWetCalls(doc: Doc): Doc {
    return utils.mapDoc(doc, (part) =>
        Array.isArray(part)
            ? part.filter(
                  (piece, i) =>
                      !(
                          isHardline(piece) &&
                          i + 1 < part.length &&
                          startsWithWet(part[i + 1])
                      ),
              )
            : part,
    );
}

export const wetChainPlugin: Plugin = {
    printers: {
        estree: {
            ...estree,
            print(path, options, print, args) {
                const printed = estree.print(path, options, print, args);
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
