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
import { FIRST_LINE_COLUMN_OFFSET } from './spanTypes';

/** What a module call's params are named and which read signals. */
export interface ModuleSiteInfo {
    positional: string[];
    /** Input class of every signal-taking param. */
    inputs: Record<string, OutputClass>;
    /** Whether the module is a method of the `.$.` namespace. */
    chainable: boolean;
}

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
    /** The output the value stands for, or null when only a run can tell. */
    output: string | null;
    /**
     * Whether writing `.output` straight after the value names that output:
     * true of a name, a member or a call that is one root, not an array.
     */
    insertable: boolean;
}

/**
 * The site a runtime record was made at. A record for an element of an array
 * that is not written as a literal belongs to the site of the whole array.
 */
export function siteOfRecord(
    sites: readonly SignalSite[],
    record: OutputResolutionRecord,
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

/** The helpers that tag a param value into a cartesian signal group. */
const SIGNAL_GROUP_HELPERS = new Set(['$g1', '$g2', '$g3']);

/** Methods of a root that return a plain collection of its `cv` output. */
const COLLECTION_BUILDERS = new Set(['slice', 'chunk']);

/**
 * `undefined`: not a root. `null`: may be one, unknown until run. Otherwise
 * the output name.
 */
type Resolved = string | null | undefined;

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
    ): void => {
        if (output !== undefined) {
            sites.push({
                ...call,
                end: value.getEnd(),
                insertable,
                output,
                slot,
                start: value.getStart(sourceFile),
            });
        }
    };

    /** Whether `.name` can be written straight after `value`. */
    const takesMember = (value: TS.Expression): boolean =>
        ts.isIdentifier(value) ||
        ts.isPropertyAccessExpression(value) ||
        ts.isElementAccessExpression(value) ||
        ts.isCallExpression(value) ||
        ts.isParenthesizedExpression(value);

    /** A value written at `slot` of the call at `call`. */
    const emitValue = (
        call: { line: number; column: number },
        slot: string,
        expression: TS.Expression,
        inputClass: OutputClass,
    ): void => {
        let value = expression;
        while (ts.isParenthesizedExpression(value)) {
            value = value.expression;
        }
        if (ts.isArrayLiteralExpression(value)) {
            value.elements.forEach((element, i) => {
                if (!ts.isSpreadElement(element)) {
                    emitValue(call, elementSlot(slot, i), element, inputClass);
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
            emitValue(call, slot, value.arguments[0], inputClass);
            return;
        }
        if (
            ts.isCallExpression(value) &&
            ts.isPropertyAccessExpression(value.expression) &&
            COLLECTION_BUILDERS.has(value.expression.name.text)
        ) {
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
                    emitValue(call, name, arg, inputClass);
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
                    emitValue(call, key, value, inputClass);
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

    /** The receiver and method path of `receiver.$.a.b`, if `callee` is one. */
    const dollarCall = (
        callee: TS.PropertyAccessExpression,
    ):
        | { receiver: TS.Expression; path: string; withMix: boolean }
        | undefined => {
        const names: string[] = [];
        let node: TS.Expression = callee;
        while (ts.isPropertyAccessExpression(node)) {
            const name = node.name.text;
            if (name === '$' || name === '$m') {
                return names.length > 0
                    ? {
                          path: names.join('.'),
                          receiver: node.expression,
                          withMix: name === '$m',
                      }
                    : undefined;
            }
            names.unshift(name);
            node = node.expression;
        }
        return undefined;
    };

    /** Whether `type` is an output or collection: it has their methods. */
    const isOutput = (type: TS.Type): boolean =>
        checker.getPropertyOfType(type, 'outMono') !== undefined;

    const receivesSignals = (type: TS.Type): boolean =>
        isUnknown(type) || isOutput(type);

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
        const dollar = dollarCall(callee);
        const chained = dollar
            ? own(tables.modules, `$${dollar.path}`)
            : undefined;
        if (dollar && chained?.chainable) {
            const receiverName = chained.positional[0];
            emitValue(
                site,
                receiverName,
                dollar.receiver,
                chained.inputs[receiverName] ?? 'cv',
            );
            const args = [...node.arguments];
            if (dollar.withMix && args.length > 0) {
                emitValue(site, MIX_SLOT, args.shift()!, 'cv');
            }
            emitModuleArgs(site, chained, args, 1);
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
            emitValue(site, RECEIVER_SLOT, callee.expression, 'cv');
            node.arguments.forEach((arg, i) => {
                if (ts.isObjectLiteralExpression(arg)) {
                    forEachField(arg, (key, value) =>
                        emitValue(
                            site,
                            optionSlot(i, key),
                            value,
                            optionClasses[key] ?? 'cv',
                        ),
                    );
                } else {
                    emitValue(site, argumentSlot(i), arg, 'cv');
                }
            });
        }
    };

    /** An element read off a root is a channel of its `cv` output. */
    const visitIndex = (node: TS.ElementAccessExpression): void => {
        const type = checker.getTypeAtLocation(node.expression);
        const outputs = outputsOf(type);
        if (outputs) {
            emit(
                position(node.argumentExpression),
                RECEIVER_SLOT,
                node.expression,
                chooseOutput(outputs, 'cv'),
                takesMember(node.expression),
            );
        }
    };

    const visit = (node: TS.Node): void => {
        if (ts.isCallExpression(node)) {
            visitCall(node);
        } else if (ts.isElementAccessExpression(node)) {
            visitIndex(node);
        }
        ts.forEachChild(node, visit);
    };
    visit(sourceFile);
    return sites;
}
