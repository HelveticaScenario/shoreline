/**
 * Slots inside a DSL API argument — an element of a poly array, a tuple
 * position, a member of an option object, however deep — read from the
 * generated DSL lib's declared types.
 */
import ts from 'typescript';

import type { DocHints } from './apiTypeReader';

/** One step from a value into a part of it: an array or tuple element, or
 *  an object member. */
export type SlotStep = { element: number } | { member: string };

/** A slot inside an argument that takes a signal, with the hints of the
 *  deepest member on its path that states any (`undefined` when its path
 *  steps through no member that does). */
export interface ApiSlot {
    hints?: DocHints;
}

/** A part of a type a step reaches: its type, or `signal` for an element of
 *  a `Poly`/`LaneSum`, with a member's own doc hints. */
interface SlotChild {
    type: ts.TypeNode | 'signal';
    hints?: DocHints;
}

/** What a slot walk asks of the type reader. */
export interface SlotReader {
    isSignal(type: ts.TypeNode): boolean;
    /** Whether `name` is a module output type, whose members are no slots. */
    isOutput(name: string): boolean;
    /** The range and default a member's own docs state, if any. */
    memberHints(member: ts.PropertySignature): DocHints | undefined;
}

const MAX_DEPTH = 16;

export class TypeSlots {
    constructor(
        private readonly file: ts.SourceFile,
        private readonly aliases: ReadonlyMap<string, ts.TypeNode>,
        private readonly interfaces: ReadonlyMap<
            string,
            ts.InterfaceDeclaration[]
        >,
        private readonly reader: SlotReader,
    ) {}

    /** The slot at `path` inside a value of `type`, or null when it takes
     *  no signal. */
    slot(
        type: ts.TypeNode | 'signal',
        path: readonly SlotStep[],
        depth = 0,
    ): ApiSlot | null {
        if (path.length === 0) {
            return type === 'signal' || this.reader.isSignal(type) ? {} : null;
        }
        if (type === 'signal' || depth > MAX_DEPTH) {
            return null;
        }
        const [step, ...rest] = path;
        for (const child of this.children(type, step, depth)) {
            const found = this.slot(child.type, rest, depth + 1);
            if (found) {
                return found.hints || !child.hints
                    ? found
                    : { hints: child.hints };
            }
        }
        return null;
    }

    /** The parts of a `type` value that `step` reaches. */
    private children(
        type: ts.TypeNode,
        step: SlotStep,
        depth: number,
    ): SlotChild[] {
        if (depth > MAX_DEPTH) {
            return [];
        }
        if (ts.isParenthesizedTypeNode(type)) {
            return this.children(type.type, step, depth + 1);
        }
        if (ts.isUnionTypeNode(type) || ts.isIntersectionTypeNode(type)) {
            return type.types.flatMap((t) => this.children(t, step, depth + 1));
        }
        if ('element' in step) {
            if (ts.isArrayTypeNode(type)) {
                return [{ type: type.elementType }];
            }
            if (ts.isTupleTypeNode(type)) {
                const el = type.elements[step.element];
                if (!el) {
                    return [];
                }
                const inner = ts.isNamedTupleMember(el) ? el.type : el;
                return [
                    {
                        type: ts.isOptionalTypeNode(inner) ? inner.type : inner,
                    },
                ];
            }
        } else if (ts.isTypeLiteralNode(type)) {
            return this.memberChildren(type.members, step.member);
        }
        if (!ts.isTypeReferenceNode(type)) {
            return [];
        }
        const name = type.typeName.getText(this.file);
        // Every element of a `Poly` or `LaneSum` also accepts a collection.
        if (name === 'Poly' || name === 'LaneSum') {
            return 'element' in step ? [{ type: 'signal' }] : [];
        }
        const arg = type.typeArguments?.[0];
        if (name === 'OrArray' && arg) {
            return 'element' in step ? [{ type: arg }] : [];
        }
        const alias = this.aliases.get(name);
        if (alias) {
            return this.children(alias, step, depth + 1);
        }
        if ('element' in step || this.reader.isOutput(name)) {
            return [];
        }
        const out: SlotChild[] = [];
        const visit = (iface: string, d: number) => {
            if (d > MAX_DEPTH) {
                return;
            }
            for (const decl of this.interfaces.get(iface) ?? []) {
                out.push(...this.memberChildren(decl.members, step.member));
                for (const clause of decl.heritageClauses ?? []) {
                    for (const base of clause.types) {
                        visit(base.expression.getText(this.file), d + 1);
                    }
                }
            }
        };
        visit(name, depth + 1);
        return out;
    }

    private memberChildren(
        list: ts.NodeArray<ts.TypeElement>,
        key: string,
    ): SlotChild[] {
        return list.flatMap((m) =>
            m.name &&
            memberName(m.name) === key &&
            ts.isPropertySignature(m) &&
            m.type
                ? [{ hints: this.reader.memberHints(m), type: m.type }]
                : [],
        );
    }
}

export function memberName(name: ts.PropertyName): string | null {
    if (
        ts.isIdentifier(name) ||
        ts.isStringLiteral(name) ||
        ts.isPrivateIdentifier(name)
    ) {
        return name.text;
    }
    return null;
}
