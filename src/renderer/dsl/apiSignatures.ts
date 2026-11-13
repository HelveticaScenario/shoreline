/**
 * Index of the DSL API's signal-typed parameters, read from the generated
 * DSL lib (`buildLibSource`), which declares every function, method, and
 * option object a patch can call.
 *
 * Functions are keyed by their dotted name (`$setOutputGain`, `$phase.bend`,
 * `$v.osc`), methods by name, and the chain modules of an audio value by
 * their dotted path (`lpf`, `mi.chorus`). Audio and video values are separate
 * families, since their methods share names (`range`, `fold`) but not
 * parameters. A parameter accepts a signal when its type reaches `Signal`,
 * `Poly`, or `LaneSum`, or unions `number` with a module output type.
 */
import ts from 'typescript';

import { hasHints, memberName, TypeReader } from './apiTypeReader';
import type { ApiMember, ApiParam, ApiSignature } from './apiTypeReader';

export type Family = 'audio' | 'video';

/** The generated interface whose methods are the chain modules. */
const CHAIN_INTERFACE = 'ModuleChain';

/** The members of one family of values. */
export interface ApiFamily {
    methods: ReadonlyMap<string, ApiSignature>;
    /** Chain modules, keyed by dotted path (`lpf`, `mi.chorus`). */
    chains: ReadonlySet<string>;
    /** Method names whose result is a video value. */
    returnsVideo: ReadonlySet<string>;
}

export interface ApiIndex {
    functions: ReadonlyMap<string, ApiSignature>;
    audio: ApiFamily;
    video: ApiFamily;
    /** Dotted names of the functions returning a video value, whether or
     *  not they take a signal. */
    videoFunctions: ReadonlySet<string>;
    /** Dotted names of declared video values, like `$v.time`. */
    videoValues: ReadonlySet<string>;
}

type Declarations = Map<string, ts.SignatureDeclarationBase[]>;

interface FamilyDeclarations {
    methods: Declarations;
    chains: Set<string>;
    returnsVideo: Set<string>;
}

const emptyFamily = (): ApiFamily => ({
    chains: new Set(),
    methods: new Map(),
    returnsVideo: new Set(),
});

const EMPTY: ApiIndex = {
    audio: emptyFamily(),
    functions: new Map(),
    video: emptyFamily(),
    videoFunctions: new Set(),
    videoValues: new Set(),
};

const cache = new Map<string, ApiIndex>();

/** The index of `libSource`, memoized on the source text. */
export function apiIndexFor(libSource: string | null): ApiIndex {
    if (!libSource) {
        return EMPTY;
    }
    let index = cache.get(libSource);
    if (!index) {
        index = indexApiSignatures(libSource);
        cache.clear();
        cache.set(libSource, index);
    }
    return index;
}

export function indexApiSignatures(libSource: string): ApiIndex {
    const file = ts.createSourceFile(
        'lib.d.ts',
        libSource,
        ts.ScriptTarget.Latest,
        true,
    );

    const aliases = new Map<string, ts.TypeNode>();
    const interfaces = new Map<string, ts.InterfaceDeclaration[]>();
    const collectTypes = (node: ts.Node) => {
        if (ts.isTypeAliasDeclaration(node) && !node.typeParameters) {
            aliases.set(node.name.text, node.type);
        } else if (ts.isInterfaceDeclaration(node)) {
            push(interfaces, node.name.text, node);
        }
        ts.forEachChild(node, collectTypes);
    };
    collectTypes(file);
    const types = new TypeReader(file, aliases, interfaces);

    const functions: Declarations = new Map();
    const families: Record<Family, FamilyDeclarations> = {
        audio: {
            chains: new Set(),
            methods: new Map(),
            returnsVideo: new Set(),
        },
        video: {
            chains: new Set(),
            methods: new Map(),
            returnsVideo: new Set(),
        },
    };
    const videoFunctions = new Set<string>();
    const videoValues = new Set<string>();

    // The chain modules: the methods of `ModuleChain` and of the namespace
    // interfaces its properties name. Module schemas describe their params.
    const chainInterfaces = new Set<string>();
    const walkChain = (prefix: string, typeName: string, depth: number) => {
        if (depth > 8) {
            return;
        }
        chainInterfaces.add(typeName);
        for (const iface of interfaces.get(typeName) ?? []) {
            for (const m of iface.members) {
                const name = m.name && memberName(m.name);
                if (!name) {
                    continue;
                }
                const key = prefix ? `${prefix}.${name}` : name;
                if (ts.isMethodSignature(m)) {
                    families.audio.chains.add(key);
                    // A chain method with no module behind it (`delay`)
                    // resolves as a method, by its declaration.
                    if (depth === 0) {
                        push(families.audio.methods, name, m);
                    }
                } else if (
                    ts.isPropertySignature(m) &&
                    m.type &&
                    ts.isTypeReferenceNode(m.type)
                ) {
                    walkChain(key, m.type.typeName.getText(file), depth + 1);
                }
            }
        }
    };
    walkChain('', CHAIN_INTERFACE, 0);

    const addFunction = (name: string, decl: ts.SignatureDeclarationBase) => {
        push(functions, name, decl);
        if (types.isVideo(decl.type, false)) {
            videoFunctions.add(name);
        }
    };
    const addMethods = (
        members: readonly (ts.TypeElement | ts.ClassElement)[],
        thisIsVideo: boolean,
    ) => {
        const family = families[thisIsVideo ? 'video' : 'audio'];
        for (const m of members) {
            const name = m.name && memberName(m.name);
            if (
                name &&
                (ts.isMethodSignature(m) || ts.isMethodDeclaration(m))
            ) {
                push(family.methods, name, m);
                if (types.isVideo(m.type, thisIsVideo)) {
                    family.returnsVideo.add(name);
                }
            }
        }
    };
    /** Members of a declared object (`$v`, `$phase`): its methods are
     *  functions, and its video-typed properties video values. */
    const addObjectMembers = (
        prefix: string,
        members: ts.NodeArray<ts.TypeElement>,
    ) => {
        for (const m of members) {
            const name = m.name && memberName(m.name);
            if (!name) {
                continue;
            }
            if (ts.isMethodSignature(m)) {
                addFunction(`${prefix}.${name}`, m);
            } else if (ts.isPropertySignature(m) && m.type) {
                if (ts.isTypeLiteralNode(m.type)) {
                    addObjectMembers(`${prefix}.${name}`, m.type.members);
                } else if (types.isVideo(m.type, false)) {
                    videoValues.add(`${prefix}.${name}`);
                }
            }
        }
    };
    const collectCallables = (node: ts.Node, ns: string) => {
        const qualify = (name: string) => (ns ? `${ns}.${name}` : name);
        if (ts.isFunctionDeclaration(node) && node.name) {
            addFunction(qualify(node.name.text), node);
        } else if (ts.isModuleDeclaration(node) && ts.isIdentifier(node.name)) {
            // `declare global` is a ModuleDeclaration named `global`.
            const inner =
                node.name.text === 'global' ? ns : qualify(node.name.text);
            if (node.body) {
                ts.forEachChild(node.body, (c) => collectCallables(c, inner));
            }
        } else if (ts.isVariableStatement(node)) {
            for (const d of node.declarationList.declarations) {
                if (!ts.isIdentifier(d.name) || !d.type) {
                    continue;
                }
                if (ts.isTypeLiteralNode(d.type)) {
                    addObjectMembers(qualify(d.name.text), d.type.members);
                } else if (types.isVideo(d.type, false)) {
                    videoValues.add(qualify(d.name.text));
                }
            }
        } else if (ts.isInterfaceDeclaration(node)) {
            if (!chainInterfaces.has(node.name.text)) {
                addMethods(
                    node.members,
                    types.isVideoInterface(node.name.text),
                );
            }
        } else if (ts.isClassDeclaration(node) && node.name) {
            addMethods(node.members, types.isVideoInterface(node.name.text));
        } else if (ts.isTypeAliasDeclaration(node)) {
            forEachTypeLiteral(node.type, (lit) =>
                addMethods(lit.members, false),
            );
        } else {
            ts.forEachChild(node, (c) => collectCallables(c, ns));
        }
    };
    collectCallables(file, '');

    const build = (decls: Declarations) => {
        const out = new Map<string, ApiSignature>();
        for (const [key, list] of decls) {
            const sig = mergeSignatures(list.map((d) => types.signature(d)));
            if (sig.some(takesSignal)) {
                out.set(key, sig);
            }
        }
        return out;
    };
    const signatures = build(functions);
    const buildFamily = (family: Family): ApiFamily => {
        const decls = families[family];
        const ofFamily = (fn: string) =>
            videoFunctions.has(fn) === (family === 'video');
        return {
            chains: decls.chains,
            methods: withFunctionHints(
                build(decls.methods),
                signatures,
                ofFamily,
            ),
            returnsVideo: decls.returnsVideo,
        };
    };
    return {
        audio: buildFamily('audio'),
        functions: signatures,
        video: buildFamily('video'),
        videoFunctions,
        videoValues,
    };
}

function push<T>(map: Map<string, T[]>, key: string, value: T) {
    const list = map.get(key);
    if (list) {
        list.push(value);
    } else {
        map.set(key, [value]);
    }
}

/**
 * Methods often document only a summary, while the function they forward to
 * (`.osc` to `$v.osc`, `.range` to `$v.range`) documents each param. A
 * param or option member without hints takes those of the same-named one on
 * the only function of `ofFamily` whose name ends in the method's name.
 */
function withFunctionHints(
    members: Map<string, ApiSignature>,
    functions: ReadonlyMap<string, ApiSignature>,
    ofFamily: (fn: string) => boolean,
): Map<string, ApiSignature> {
    const byLastName = new Map<string, ApiSignature | null>();
    for (const [key, sig] of functions) {
        if (ofFamily(key)) {
            const last = lastSegment(key);
            byLastName.set(last, byLastName.has(last) ? null : sig);
        }
    }
    const out = new Map<string, ApiSignature>();
    for (const [key, sig] of members) {
        const fn = byLastName.get(lastSegment(key));
        const byName = new Map(fn?.map((p) => [p.name, p]));
        out.set(
            key,
            sig.map((p) => {
                const source = byName.get(p.name);
                return source ? withHintsOf(p, source) : p;
            }),
        );
    }
    return out;
}

function withHintsOf(param: ApiParam, source: ApiParam): ApiParam {
    let options = param.options;
    if (options && source.options) {
        const merged = new Map(options);
        for (const [key, member] of options) {
            const from = source.options.get(key);
            if (from && !hasHints(member.hints)) {
                merged.set(key, { ...member, hints: from.hints });
            }
        }
        options = merged;
    }
    return {
        ...param,
        hints: hasHints(param.hints) ? param.hints : source.hints,
        options,
    };
}

function lastSegment(key: string): string {
    return key.slice(key.lastIndexOf('.') + 1);
}

function takesSignal(p: ApiParam): boolean {
    return p.signal || [...(p.options?.values() ?? [])].some((m) => m.signal);
}

/**
 * Merge overloads and same-named declarations: a parameter or member takes a
 * signal only when every declaration that has it says so.
 */
function mergeSignatures(sigs: ApiParam[][]): ApiParam[] {
    const length = Math.max(...sigs.map((s) => s.length));
    const merged: ApiParam[] = [];
    for (let i = 0; i < length; i++) {
        const at = sigs.map((s) => s[i]).filter((p) => p !== undefined);
        let options: Map<string, ApiMember> | null = null;
        for (const p of at) {
            for (const [key, member] of p.options ?? []) {
                options ??= new Map();
                const prev = options.get(key);
                options.set(
                    key,
                    prev
                        ? {
                              hints: prev.hints,
                              signal: prev.signal && member.signal,
                          }
                        : member,
                );
            }
        }
        merged.push({
            hints: at[0].hints,
            name: at[0].name,
            options,
            rest: at.some((p) => p.rest),
            signal: at.every((p) => p.signal),
        });
    }
    return merged;
}

function forEachTypeLiteral(
    type: ts.TypeNode,
    fn: (lit: ts.TypeLiteralNode) => void,
) {
    if (ts.isTypeLiteralNode(type)) {
        fn(type);
    } else if (ts.isIntersectionTypeNode(type) || ts.isUnionTypeNode(type)) {
        type.types.forEach((t) => forEachTypeLiteral(t, fn));
    } else if (ts.isParenthesizedTypeNode(type)) {
        forEachTypeLiteral(type.type, fn);
    }
}
