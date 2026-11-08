/**
 * Reads the generated DSL lib's declarations: which parameter and member
 * types accept a signal, which option-object members a type offers, which
 * types are video values, and the range and default their JSDoc states.
 */
import ts from 'typescript';

/** A parameter or option member's doc-stated range and default, if any. */
export interface DocHints {
    range?: { min: number; max: number };
    defaultValue?: number;
}

export interface ApiMember {
    signal: boolean;
    hints: DocHints;
}

export interface ApiParam {
    name: string;
    /** Takes a signal as the argument itself. */
    signal: boolean;
    rest: boolean;
    hints: DocHints;
    /** Members of an option object this parameter accepts, by key. */
    options: ReadonlyMap<string, ApiMember> | null;
}

export type ApiSignature = readonly ApiParam[];

const SIGNAL_ALIASES = new Set(['Signal', 'Poly', 'Mono']);
const OUTPUT_TYPES = new Set([
    'ModuleOutput',
    'Collection',
    'CollectionWithRange',
    'ModuleOutputWithRange',
]);
const MAX_DEPTH = 16;
const NUM = String.raw`[-−]?\d+(?:\.\d+)?`;
const RANGE_RE = new RegExp(String.raw`(${NUM})\s*(?:–|\.\.)\s*(${NUM})`);
const SYMMETRIC_RE = new RegExp(String.raw`±\s*(${NUM})`);
const DEFAULT_RE = new RegExp(
    String.raw`\bdefault(?:s to)?:?\s+(${NUM})(?![\w.])`,
    'i',
);

export class TypeReader {
    private readonly signalMemo = new Map<string, boolean>();
    /** The `Video*` interfaces and every interface extending one. */
    private readonly videoInterfaces: ReadonlySet<string>;

    constructor(
        private readonly file: ts.SourceFile,
        private readonly aliases: ReadonlyMap<string, ts.TypeNode>,
        private readonly interfaces: ReadonlyMap<
            string,
            ts.InterfaceDeclaration[]
        >,
    ) {
        const video = new Set(
            [...interfaces.keys()].filter((n) => n.startsWith('Video')),
        );
        for (let grew = true; grew; ) {
            grew = false;
            for (const [name, list] of interfaces) {
                if (
                    !video.has(name) &&
                    list.some((i) => this.extendsAny(i, video))
                ) {
                    video.add(name);
                    grew = true;
                }
            }
        }
        this.videoInterfaces = video;
    }

    signature(decl: ts.SignatureDeclarationBase): ApiParam[] {
        const paramDocs = paramDocTexts(decl);
        return decl.parameters.map((p) => {
            const name = ts.isIdentifier(p.name) ? p.name.text : '';
            const rest = p.dotDotDotToken !== undefined;
            // A rest parameter's type is the array of what each argument takes.
            const type =
                rest && p.type && ts.isArrayTypeNode(p.type)
                    ? p.type.elementType
                    : p.type;
            return {
                hints: docHints(paramDocs.get(name) ?? ''),
                name,
                // `@param config.zoom` documents the `zoom` member of `config`.
                options: type
                    ? this.options(type, (key) =>
                          paramDocs.get(`${name}.${key}`),
                      )
                    : null,
                rest,
                signal: type ? this.isSignal(type) : false,
            };
        });
    }

    isSignal(type: ts.TypeNode, depth = 0): boolean {
        if (depth > MAX_DEPTH) {
            return false;
        }
        if (ts.isParenthesizedTypeNode(type)) {
            return this.isSignal(type.type, depth + 1);
        }
        if (ts.isUnionTypeNode(type)) {
            const hasNumber = type.types.some(
                (t) => t.kind === ts.SyntaxKind.NumberKeyword,
            );
            const hasOutput = type.types.some(
                (t) =>
                    ts.isTypeReferenceNode(t) &&
                    OUTPUT_TYPES.has(t.typeName.getText(this.file)),
            );
            return (
                (hasNumber && hasOutput) ||
                type.types.some((t) => this.isSignal(t, depth + 1))
            );
        }
        if (ts.isTypeReferenceNode(type)) {
            const name = type.typeName.getText(this.file);
            if (SIGNAL_ALIASES.has(name)) {
                return true;
            }
            const memo = this.signalMemo.get(name);
            if (memo !== undefined) {
                return memo;
            }
            const alias = this.aliases.get(name);
            this.signalMemo.set(name, false);
            const result = alias ? this.isSignal(alias, depth + 1) : false;
            this.signalMemo.set(name, result);
            return result;
        }
        return false;
    }

    isVideoInterface(name: string): boolean {
        return this.videoInterfaces.has(name);
    }

    /** Whether `type` is a video value; `this` is one inside a video
     *  interface. */
    isVideo(type: ts.TypeNode | undefined, thisIsVideo: boolean): boolean {
        if (!type) {
            return false;
        }
        if (type.kind === ts.SyntaxKind.ThisType) {
            return thisIsVideo;
        }
        if (ts.isParenthesizedTypeNode(type)) {
            return this.isVideo(type.type, thisIsVideo);
        }
        if (ts.isUnionTypeNode(type) || ts.isIntersectionTypeNode(type)) {
            return type.types.some((t) => this.isVideo(t, thisIsVideo));
        }
        return (
            ts.isTypeReferenceNode(type) &&
            this.videoInterfaces.has(type.typeName.getText(this.file))
        );
    }

    /**
     * Members of the option objects `type` accepts, or null for none. A
     * member without docs of its own takes `memberDoc(key)`.
     */
    private options(
        type: ts.TypeNode,
        memberDoc: (key: string) => string | undefined,
    ): Map<string, ApiMember> | null {
        const members = new Map<string, ApiMember>();
        const visit = (t: ts.TypeNode, depth: number) => {
            if (depth > MAX_DEPTH) {
                return;
            }
            if (ts.isParenthesizedTypeNode(t)) {
                visit(t.type, depth + 1);
            } else if (ts.isUnionTypeNode(t) || ts.isIntersectionTypeNode(t)) {
                t.types.forEach((u) => visit(u, depth + 1));
            } else if (ts.isTypeLiteralNode(t)) {
                this.addMembers(t.members, members, memberDoc);
            } else if (ts.isTypeReferenceNode(t)) {
                this.addInterface(
                    t.typeName.getText(this.file),
                    members,
                    memberDoc,
                    depth + 1,
                );
            }
        };
        visit(type, 0);
        return members.size > 0 ? members : null;
    }

    /** Own members first, so an override shadows the inherited member. */
    private addInterface(
        name: string,
        into: Map<string, ApiMember>,
        memberDoc: (key: string) => string | undefined,
        depth: number,
    ) {
        if (depth > MAX_DEPTH || OUTPUT_TYPES.has(name)) {
            return;
        }
        for (const iface of this.interfaces.get(name) ?? []) {
            this.addMembers(iface.members, into, memberDoc);
            for (const clause of iface.heritageClauses ?? []) {
                for (const base of clause.types) {
                    this.addInterface(
                        base.expression.getText(this.file),
                        into,
                        memberDoc,
                        depth + 1,
                    );
                }
            }
        }
    }

    private addMembers(
        list: ts.NodeArray<ts.TypeElement>,
        into: Map<string, ApiMember>,
        memberDoc: (key: string) => string | undefined,
    ) {
        for (const m of list) {
            const name = m.name && memberName(m.name);
            if (!name || !ts.isPropertySignature(m) || into.has(name)) {
                continue;
            }
            const own = docHints(jsDocText(m));
            into.set(name, {
                hints: hasHints(own) ? own : docHints(memberDoc(name) ?? ''),
                signal: m.type ? this.isSignal(m.type) : false,
            });
        }
    }

    private extendsAny(
        iface: ts.InterfaceDeclaration,
        names: ReadonlySet<string>,
    ): boolean {
        return (iface.heritageClauses ?? []).some((h) =>
            h.types.some((t) => names.has(t.expression.getText(this.file))),
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

export function hasHints(hints: DocHints): boolean {
    return hints.range !== undefined || hints.defaultValue !== undefined;
}

function jsDocText(node: ts.Node): string {
    return ts
        .getJSDocCommentsAndTags(node)
        .map((d) => (ts.isJSDoc(d) ? ts.getTextOfJSDocComment(d.comment) : ''))
        .join(' ');
}

/** `@param` text by parameter name (`config.zoom` for a member), falling
 *  back to the summary for an undocumented first parameter. */
function paramDocTexts(decl: ts.SignatureDeclarationBase): Map<string, string> {
    const docs = new Map<string, string>();
    for (const tag of ts.getJSDocTags(decl)) {
        if (ts.isJSDocParameterTag(tag)) {
            docs.set(
                entityName(tag.name),
                ts.getTextOfJSDocComment(tag.comment) ?? '',
            );
        }
    }
    const first = decl.parameters[0];
    if (first && ts.isIdentifier(first.name) && !docs.has(first.name.text)) {
        docs.set(first.name.text, jsDocText(decl));
    }
    return docs;
}

function entityName(name: ts.EntityName): string {
    return ts.isIdentifier(name)
        ? name.text
        : `${entityName(name.left)}.${name.right.text}`;
}

const num = (s: string) => Number(s.replace('−', '-'));

/** A `lo–hi` / `lo..hi` / `±n` range and a `default N` stated in doc
 *  text. */
export function docHints(text: string): DocHints {
    const hints: DocHints = {};
    const range = RANGE_RE.exec(text);
    const symmetric = SYMMETRIC_RE.exec(text);
    if (range) {
        const [min, max] = [num(range[1]), num(range[2])];
        if (min < max) {
            hints.range = { max, min };
        }
    } else if (symmetric) {
        const max = num(symmetric[1]);
        if (max > 0) {
            hints.range = { max, min: -max };
        }
    }
    const def = DEFAULT_RE.exec(text);
    if (def) {
        hints.defaultValue = num(def[1]);
    }
    return hints;
}
