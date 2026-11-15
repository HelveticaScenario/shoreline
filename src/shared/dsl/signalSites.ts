/**
 * Finds every place in a patch where a value may be a multi-output module
 * root, and resolves the ones the type checker can name. The editor shows a
 * hint at each site; the runtime's resolution records fill in the sites left
 * unresolved here.
 *
 * Takes the `typescript` namespace as an argument: it runs inside Monaco's
 * TypeScript worker against the worker's own compiler.
 */

import type * as TS from 'typescript';
import {
    argumentSlot,
    arraySlotOf,
    chooseOutput,
    elementSlot,
    MIX_SLOT,
    optionSlot,
    RECEIVER_SLOT,
    type OutputClass,
    type OutputResolutionRecord,
    type ResolvableOutput,
} from './outputResolutionRule';
import { type Channels, collectionChannels, liftedChannels } from './channels';
import { FIRST_LINE_COLUMN_OFFSET } from './spanTypes';

/** What a module call's params are named and which read signals. */
export interface ModuleSiteInfo {
    positional: string[];
    /** Input class of every signal-taking param. */
    inputs: Record<string, OutputClass>;
    /** Whether the module is a chain method of collections. */
    chainable: boolean;
    /** How the module's lanes carry stereo (`ModuleSchema.stereo`). */
    stereo: 'lift' | 'mono' | 'stereo';
    /** The inputs that fold a stereo signal to mono. */
    folds: string[];
    /** The `$buffer` params whose channels the module lifts with, as with an input. */
    buffers: string[];
    /** Outputs that carry one value per lane, mono whatever the module is. */
    monoOutputs: string[];
}

/**
 * What a global gives: its channels, or `collection` for a builder whose
 * lanes are its arguments (`$c`).
 */
export type GlobalChannels = Channels | 'collection';

/**
 * Methods on an output or collection that read it, and the signals among
 * their arguments, as `cv`. Each lists the fields of its options object that
 * read another class.
 */
export type ChainMethods = Readonly<
    Record<string, Readonly<Record<string, OutputClass>>>
>;

/** Everything about the DSL the collector needs; plain data, built in main. */
export interface SignalSiteTables {
    /** Outputs of each root type, keyed by its qualified type name. */
    roots: Record<string, ResolvableOutput[]>;
    /** Module calls, keyed by dotted call name (`$tr808.kick`). */
    modules: Record<string, ModuleSiteInfo>;
    chainMethods: ChainMethods;
    /** DSL globals that are, or make, signals other than module calls. */
    globals: Record<string, GlobalChannels>;
    /** Qualified type names of values that are always mono, such as patterns. */
    monoTypes: string[];
}

/** One value position in a patch. */
export interface SignalSite {
    /** Call site as V8 reports it for the document: 1-based line and column. */
    line: number;
    column: number;
    /** Matches `OutputResolutionRecord.slot`. */
    slot: string;
    /** Document offsets of the value. Its hint goes at `end`. */
    start: number;
    end: number;
    /**
     * The output the value stands for, or null when only a run can tell;
     * absent when the value is not a module root.
     */
    output?: string | null;
    /**
     * Whether the value reaches an input that folds it to mono while it may
     * be stereo, so `.mono()` belongs after it.
     */
    fold: boolean;
    /**
     * Whether writing `._<name>` or `.mono()` straight after the value
     * applies to it: true of a name, a member or a call, not an array.
     */
    insertable: boolean;
}

/**
 * The site a runtime record was made at. A record for an element of an array
 * that is not written as a literal belongs to the site of the whole array.
 */
export function siteOfRecord(
    sites: readonly SignalSite[],
    record: Pick<OutputResolutionRecord, 'line' | 'column' | 'slot'>,
): SignalSite | undefined {
    const column =
        record.line === 1
            ? record.column - FIRST_LINE_COLUMN_OFFSET
            : record.column;
    const atCall = sites.filter(
        (s) => s.line === record.line && s.column === column,
    );
    for (
        let slot: string | undefined = record.slot;
        slot !== undefined;
        slot = arraySlotOf(slot)
    ) {
        const site = atCall.find((s) => s.slot === slot);
        if (site) {
            return site;
        }
    }
    return undefined;
}

/**
 * The options of each out that fold a stereo signal: a stereo out's pan
 * and spread go to `$pan`'s, and a mono out folds its gain.
 */
const FOLDING_OPTIONS: Readonly<Record<string, readonly string[]>> = {
    out: ['pan', 'spread'],
    outMono: ['gain'],
};

/** The helpers that tag a param value into a cartesian signal group. */
const SIGNAL_GROUP_HELPERS = new Set(['$g1', '$g2', '$g3']);

/**
 * `undefined`: not a root. `null`: may be one, unknown until run. Otherwise
 * the output name.
 */
type Resolved = string | null | undefined;

/**
 * Every value that is a module root or folds to mono where it is read. Whether
 * a value may be stereo is told from the schema as the patch is written: a
 * module's stereo mode and its inputs' channels, through `const` bindings.
 */
export function collectSignalSites(
    ts: typeof TS,
    sourceFile: TS.SourceFile,
    checker: TS.TypeChecker,
    tables: SignalSiteTables,
): SignalSite[] {
    const sites: SignalSite[] = [];

    /** `table[key]`, when the table itself holds that key. */
    const own = <V>(table: Record<string, V>, key: string): V | undefined =>
        Object.hasOwn(table, key) ? table[key] : undefined;

    /** The option classes of chain method `name`, if it is one. */
    const chainMethod = (name: string) => own(tables.chainMethods, name);

    const qualifiedName = (symbol: TS.Symbol): string => {
        let name = symbol.name;
        let node: TS.Node | undefined = symbol.declarations?.[0]?.parent;
        while (node) {
            if (
                ts.isModuleDeclaration(node) &&
                !(node.flags & ts.NodeFlags.GlobalAugmentation)
            ) {
                name = `${node.name.text}.${name}`;
            }
            node = node.parent;
        }
        return name;
    };

    const outputsOf = (type: TS.Type): ResolvableOutput[] | undefined => {
        const symbol = type.aliasSymbol ?? type.getSymbol();
        return symbol ? own(tables.roots, qualifiedName(symbol)) : undefined;
    };

    const isUnknown = (type: TS.Type): boolean =>
        (type.flags & (ts.TypeFlags.Any | ts.TypeFlags.Unknown)) !== 0;

    const resolveType = (type: TS.Type, inputClass: OutputClass): Resolved => {
        if (isUnknown(type)) {
            return null;
        }
        const outputs = outputsOf(type);
        if (outputs) {
            return chooseOutput(outputs, inputClass);
        }
        let members: readonly TS.Type[] | undefined;
        if (type.isUnion()) {
            members = type.types.filter(
                (t) =>
                    !(t.flags & (ts.TypeFlags.Undefined | ts.TypeFlags.Null)),
            );
        } else if (checker.isArrayType(type) || checker.isTupleType(type)) {
            members = checker.getTypeArguments(type as TS.TypeReference);
        }
        if (!members || members.length === 0) {
            return undefined;
        }
        const resolved = members.map((t) => resolveType(t, inputClass));
        if (resolved.every((r) => r === undefined)) {
            return undefined;
        }
        // Members that disagree, or mix roots with plain values, differ per
        // element at run time.
        return resolved.every((r) => r === resolved[0]) ? resolved[0] : null;
    };

    const position = (node: TS.Node): { line: number; column: number } => {
        const { line, character } = sourceFile.getLineAndCharacterOfPosition(
            node.getStart(sourceFile),
        );
        return { column: character + 1, line: line + 1 };
    };

    const emit = (
        call: { line: number; column: number },
        slot: string,
        value: TS.Node,
        output: Resolved,
        insertable: boolean,
        fold: boolean,
    ): void => {
        if (output !== undefined || fold) {
            sites.push({
                ...call,
                end: value.getEnd(),
                fold,
                insertable,
                ...(output !== undefined && { output }),
                slot,
                start: value.getStart(sourceFile),
            });
        }
    };

    /** Whether `.name` can be written straight after `value`; a shorthand `{ key }` cannot take one. */
    const takesMember = (value: TS.Expression): boolean =>
        (ts.isIdentifier(value) &&
            !ts.isShorthandPropertyAssignment(value.parent)) ||
        ts.isPropertyAccessExpression(value) ||
        ts.isElementAccessExpression(value) ||
        ts.isCallExpression(value) ||
        ts.isTaggedTemplateExpression(value) ||
        ts.isParenthesizedExpression(value);

    /**
     * A value written at `slot` of the call at `call`, read by an input that
     * folds a stereo signal when `folding`.
     */
    const emitValue = (
        call: { line: number; column: number },
        slot: string,
        expression: TS.Expression,
        inputClass: OutputClass,
        folding = false,
    ): void => {
        let value = expression;
        while (ts.isParenthesizedExpression(value)) {
            value = value.expression;
        }
        if (ts.isArrayLiteralExpression(value)) {
            value.elements.forEach((element, i) => {
                if (!ts.isSpreadElement(element)) {
                    emitValue(
                        call,
                        elementSlot(slot, i),
                        element,
                        inputClass,
                        folding,
                    );
                }
            });
            return;
        }
        if (
            ts.isCallExpression(value) &&
            ts.isIdentifier(value.expression) &&
            SIGNAL_GROUP_HELPERS.has(value.expression.text) &&
            value.arguments.length === 1
        ) {
            emitValue(call, slot, value.arguments[0], inputClass, folding);
            return;
        }
        const type = checker.getTypeAtLocation(value);
        emit(
            call,
            slot,
            expression,
            resolveType(type, inputClass),
            takesMember(expression) &&
                !checker.isArrayType(type) &&
                !checker.isTupleType(type),
            folding && channelsOf(value) !== 'mono',
        );
    };

    /** Each `key: value` and shorthand `key` of an object literal. */
    const forEachField = (
        literal: TS.ObjectLiteralExpression,
        visitField: (key: string, value: TS.Expression) => void,
    ): void => {
        for (const property of literal.properties) {
            if (
                ts.isPropertyAssignment(property) &&
                ts.isIdentifier(property.name)
            ) {
                visitField(property.name.text, property.initializer);
            } else if (ts.isShorthandPropertyAssignment(property)) {
                visitField(property.name.text, property.name);
            }
        }
    };

    /** Arguments of a module call, `first` being the first positional they fill. */
    const emitModuleArgs = (
        call: { line: number; column: number },
        info: ModuleSiteInfo,
        args: readonly TS.Expression[],
        first: number,
    ): void => {
        const spread = args.findIndex((arg) => ts.isSpreadElement(arg));
        const placed = spread === -1 ? args : args.slice(0, spread);
        placed.forEach((arg, i) => {
            const index = first + i;
            if (index < info.positional.length) {
                const name = info.positional[index];
                const inputClass = info.inputs[name];
                if (inputClass) {
                    emitValue(
                        call,
                        name,
                        arg,
                        inputClass,
                        info.folds.includes(name),
                    );
                }
                return;
            }
            if (
                index !== info.positional.length ||
                !ts.isObjectLiteralExpression(arg)
            ) {
                return;
            }
            forEachField(arg, (key, value) => {
                const inputClass = info.inputs[key];
                if (inputClass) {
                    emitValue(
                        call,
                        key,
                        value,
                        inputClass,
                        info.folds.includes(key),
                    );
                }
            });
        });
    };

    /** `$saw` or `$tr808.kick` as written, when the callee is a dotted name. */
    const dottedName = (expression: TS.Expression): string | undefined => {
        if (ts.isIdentifier(expression)) {
            return expression.text;
        }
        if (ts.isPropertyAccessExpression(expression)) {
            const head = dottedName(expression.expression);
            return head ? `${head}.${expression.name.text}` : undefined;
        }
        return undefined;
    };

    /** Whether `type` is an output or collection: it has their methods. */
    const isOutput = (type: TS.Type): boolean =>
        checker.getPropertyOfType(type, 'outMono') !== undefined;

    const receivesSignals = (type: TS.Type): boolean =>
        isUnknown(type) || isOutput(type);

    /**
     * The receiver and chain module of `receiver.a.b`, if `callee` is a
     * chain call: the longest trailing path that names a chainable module on
     * a receiver that reads signals.
     */
    const chainCall = (
        callee: TS.PropertyAccessExpression,
    ): { receiver: TS.Expression; module: ModuleSiteInfo } | undefined => {
        const names: string[] = [];
        let found:
            | { receiver: TS.Expression; module: ModuleSiteInfo }
            | undefined;
        let node: TS.Expression = callee;
        while (ts.isPropertyAccessExpression(node)) {
            names.unshift(node.name.text);
            node = node.expression;
            const module = own(tables.modules, `$${names.join('.')}`);
            if (
                module?.chainable &&
                receivesSignals(checker.getTypeAtLocation(node))
            ) {
                found = { module, receiver: node };
            }
        }
        return found;
    };

    /**
     * Whether `type` is always mono: plain data (numbers, strings, booleans,
     * `undefined`, and arrays of them) or a type the tables name mono.
     */
    const isPlain = (type: TS.Type): boolean => {
        if (
            type.flags &
            (ts.TypeFlags.NumberLike |
                ts.TypeFlags.StringLike |
                ts.TypeFlags.BooleanLike |
                ts.TypeFlags.Undefined |
                ts.TypeFlags.Null |
                ts.TypeFlags.Void)
        ) {
            return true;
        }
        const symbol = type.aliasSymbol ?? type.getSymbol();
        if (symbol && tables.monoTypes.includes(qualifiedName(symbol))) {
            return true;
        }
        if (type.isUnion()) {
            return type.types.every(isPlain);
        }
        if (checker.isArrayType(type) || checker.isTupleType(type)) {
            return checker
                .getTypeArguments(type as TS.TypeReference)
                .every(isPlain);
        }
        return false;
    };

    /** The `const` declarations being followed, so a cycle reads as unknown. */
    const following = new Set<TS.Node>();

    /** Each expression's channels once told, as a `const` is read many times. */
    const told = new Map<TS.Node, Channels>();

    /** What `expression` carries, mono, stereo or unknown, told without running it. */
    const channelsOf = (expression: TS.Expression): Channels => {
        let channels = told.get(expression);
        if (channels === undefined) {
            channels = tellChannels(expression);
            told.set(expression, channels);
        }
        return channels;
    };

    const tellChannels = (expression: TS.Expression): Channels => {
        let value = expression;
        while (
            ts.isParenthesizedExpression(value) ||
            ts.isAsExpression(value) ||
            ts.isNonNullExpression(value) ||
            ts.isSatisfiesExpression(value)
        ) {
            value = value.expression;
        }
        if (
            ts.isLiteralExpression(value) ||
            ts.isTemplateExpression(value) ||
            ts.isTaggedTemplateExpression(value) ||
            isPlain(checker.getTypeAtLocation(value))
        ) {
            return 'mono';
        }
        if (ts.isArrayLiteralExpression(value)) {
            return collectionChannels(
                value.elements.map((element) =>
                    channelsOf(
                        ts.isSpreadElement(element)
                            ? element.expression
                            : element,
                    ),
                ),
            );
        }
        if (ts.isIdentifier(value)) {
            return identifierChannels(value);
        }
        // An output, a lane or a slice carries its module's channels, but a
        // `mono` output is mono.
        if (
            ts.isPropertyAccessExpression(value) &&
            value.name.text.startsWith('_') &&
            isMonoOutput(value.expression, value.name.text.slice(1))
        ) {
            return 'mono';
        }
        if (
            ts.isPropertyAccessExpression(value) ||
            ts.isElementAccessExpression(value)
        ) {
            return channelsOf(value.expression);
        }
        if (ts.isCallExpression(value)) {
            return callChannels(value);
        }
        return 'unknown';
    };

    /** Chain methods that hand back their receiver. */
    const RETURNS_RECEIVER = new Set(['out', 'outMono', 'scope', 'send']);

    /**
     * Whether `expression._port` is a `mono` output: `expression` is a module
     * call, through parentheses, `const` bindings, `.concat(...)` and chain
     * methods that hand back their receiver.
     */
    const isMonoOutput = (expression: TS.Expression, port: string): boolean => {
        let value = expression;
        while (
            ts.isParenthesizedExpression(value) ||
            ts.isAsExpression(value) ||
            ts.isNonNullExpression(value) ||
            ts.isSatisfiesExpression(value)
        ) {
            value = value.expression;
        }
        if (ts.isIdentifier(value)) {
            const declaration =
                checker.getSymbolAtLocation(value)?.valueDeclaration;
            if (
                !declaration ||
                !ts.isVariableDeclaration(declaration) ||
                !declaration.initializer ||
                declaration.getSourceFile() !== sourceFile ||
                !(ts.getCombinedNodeFlags(declaration) & ts.NodeFlags.Const) ||
                following.has(declaration)
            ) {
                return false;
            }
            following.add(declaration);
            try {
                return isMonoOutput(declaration.initializer, port);
            } finally {
                following.delete(declaration);
            }
        }
        if (!ts.isCallExpression(value)) {
            return false;
        }
        const callee = value.expression;
        const name = dottedName(callee);
        const module = name ? own(tables.modules, name) : undefined;
        if (module) {
            return module.monoOutputs.includes(port);
        }
        if (!ts.isPropertyAccessExpression(callee)) {
            return false;
        }
        const chain = chainCall(callee);
        if (chain) {
            return chain.module.monoOutputs.includes(port);
        }
        if (RETURNS_RECEIVER.has(callee.name.text)) {
            return isMonoOutput(callee.expression, port);
        }
        // A multi-output module's outputs join output by output.
        if (callee.name.text === 'concat') {
            return [callee.expression, ...value.arguments].every(
                (part) => !ts.isSpreadElement(part) && isMonoOutput(part, port),
            );
        }
        return false;
    };

    const identifierChannels = (name: TS.Identifier): Channels => {
        const symbol = checker.getSymbolAtLocation(name);
        const declaration = symbol?.valueDeclaration;
        if (
            declaration &&
            ts.isVariableDeclaration(declaration) &&
            declaration.initializer &&
            declaration.getSourceFile() === sourceFile &&
            ts.getCombinedNodeFlags(declaration) & ts.NodeFlags.Const &&
            !following.has(declaration)
        ) {
            following.add(declaration);
            try {
                return channelsOf(declaration.initializer);
            } finally {
                following.delete(declaration);
            }
        }
        if (declaration && declaration.getSourceFile() !== sourceFile) {
            const global = own(tables.globals, name.text);
            if (global && global !== 'collection') {
                return global;
            }
        }
        return 'unknown';
    };

    /** The channels of a module given its arguments, `first` being the positional they start at. */
    const moduleChannels = (
        info: ModuleSiteInfo,
        args: readonly TS.Expression[],
        first: number,
        receiver?: TS.Expression,
    ): Channels => {
        if (info.stereo === 'mono' || info.stereo === 'stereo') {
            return info.stereo;
        }
        const inputs: Channels[] = receiver ? [channelsOf(receiver)] : [];
        args.forEach((arg, i) => {
            const index = first + i;
            if (ts.isSpreadElement(arg)) {
                inputs.push('unknown');
            } else if (index < info.positional.length) {
                const name = info.positional[index];
                if (info.inputs[name] || info.buffers.includes(name)) {
                    inputs.push(channelsOf(arg));
                }
            } else if (ts.isObjectLiteralExpression(arg)) {
                forEachField(arg, (key, field) => {
                    if (info.inputs[key] || info.buffers.includes(key)) {
                        inputs.push(channelsOf(field));
                    }
                });
            } else if (!isPlain(checker.getTypeAtLocation(arg))) {
                inputs.push('unknown');
            }
        });
        return liftedChannels(inputs);
    };

    const callChannels = (node: TS.CallExpression): Channels => {
        const callee = node.expression;
        const name = dottedName(callee);
        const module = name ? own(tables.modules, name) : undefined;
        if (module) {
            return moduleChannels(module, node.arguments, 0);
        }
        const global =
            ts.isIdentifier(callee) && own(tables.globals, callee.text);
        if (global === 'collection') {
            return collectionChannels(node.arguments.map(channelsOf));
        }
        if (global) {
            return global;
        }
        if (!ts.isPropertyAccessExpression(callee)) {
            return 'unknown';
        }
        const chain = chainCall(callee);
        if (chain) {
            return moduleChannels(
                chain.module,
                node.arguments,
                1,
                chain.receiver,
            );
        }
        if (RETURNS_RECEIVER.has(callee.name.text)) {
            return channelsOf(callee.expression);
        }
        // `.range()` builds a `$remap`, which lifts with its bounds.
        if (callee.name.text === 'range') {
            return liftedChannels([
                channelsOf(callee.expression),
                ...node.arguments.map(channelsOf),
            ]);
        }
        if (callee.name.text === 'concat') {
            return collectionChannels([
                channelsOf(callee.expression),
                ...node.arguments.map(channelsOf),
            ]);
        }
        return 'unknown';
    };

    const visitCall = (node: TS.CallExpression): void => {
        const callee = node.expression;
        const name = dottedName(callee);
        const module = name ? own(tables.modules, name) : undefined;
        if (module) {
            const site = position(
                ts.isPropertyAccessExpression(callee) ? callee.name : callee,
            );
            emitModuleArgs(site, module, node.arguments, 0);
            return;
        }
        if (!ts.isPropertyAccessExpression(callee)) {
            return;
        }
        const site = position(callee.name);
        const chain = chainCall(callee);
        if (chain) {
            const receiverName = chain.module.positional[0];
            emitValue(
                site,
                receiverName,
                chain.receiver,
                chain.module.inputs[receiverName] ?? 'cv',
                chain.module.folds.includes(receiverName),
            );
            emitModuleArgs(site, chain.module, node.arguments, 1);
            return;
        }
        if (
            callee.name.text === 'pipeMix' &&
            node.arguments.length > 1 &&
            receivesSignals(checker.getTypeAtLocation(callee.expression))
        ) {
            emitValue(site, MIX_SLOT, node.arguments[1], 'cv');
            return;
        }
        const optionClasses = chainMethod(callee.name.text);
        if (
            optionClasses &&
            receivesSignals(checker.getTypeAtLocation(callee.expression))
        ) {
            const method = callee.name.text;
            const folding = own(FOLDING_OPTIONS, method) ?? [];
            // A mono out folds what it sends.
            emitValue(
                site,
                RECEIVER_SLOT,
                callee.expression,
                'cv',
                method === 'outMono',
            );
            node.arguments.forEach((arg, i) => {
                if (ts.isObjectLiteralExpression(arg)) {
                    forEachField(arg, (key, value) =>
                        emitValue(
                            site,
                            optionSlot(i, key),
                            value,
                            optionClasses[key] ?? 'cv',
                            folding.includes(key),
                        ),
                    );
                } else {
                    emitValue(
                        site,
                        argumentSlot(i),
                        arg,
                        'cv',
                        method === 'outMono' && i === 1,
                    );
                }
            });
        }
    };

    const visit = (node: TS.Node): void => {
        if (ts.isCallExpression(node)) {
            visitCall(node);
        }
        ts.forEachChild(node, visit);
    };
    visit(sourceFile);
    return sites;
}
