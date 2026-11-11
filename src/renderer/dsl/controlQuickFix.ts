/**
 * Quick-fix analysis for turning DSL params into Control panel controls.
 *
 * Given a patch source and a cursor offset, produces the actions the editor
 * can offer there: replacing an existing literal argument with a control, or
 * adding a control for a signal param the call doesn't set yet. Every API
 * call resolves — module factories and chains, functions, and signal methods
 * (paramCallResolver.ts) — and which control, and its label, range, and
 * initial value, come from the param's metadata (paramControls.ts).
 *
 * Every fix is checked to add no parse error to the source; formatting the
 * edited statement happens separately (controlFixFormat.ts), only for the
 * fix the user picks.
 *
 * Pure and renderer-safe: no Monaco or Electron imports, so the analysis is
 * unit-testable against plain source strings.
 */
import ts from 'typescript';

import { propertyName } from '../../shared/dsl/ast/names';
import { parseDsl } from '../../shared/dsl/ast/parse';
import { findControlCalls } from '../../shared/dsl/controlCalls';
import type { Schemas } from '../../shared/dsl/schemaTypeResolver';
import { hintedRef } from './apiParamControls';
import type { ParamRef } from './apiParamControls';
import type { ApiIndex } from './apiSignatures';
import { addFixes, dedupeLabel } from './controlAddFixes';
import type { CodeStyle, TextEdit } from './listEdits';
import { introducesSyntaxErrors, parseErrors } from './syntaxCheck';
import { asLiteral, createCallResolver } from './paramCallResolver';
import { wrapChoices } from './paramControls';
import type { ParamLiteral } from './paramControls';

export interface ControlQuickFix extends TextEdit {
    title: string;
    kind: 'wrap' | 'add' | 'addAll' | 'unwrap';
    /** Offset within newText where the inserted control call begins. */
    controlOffset: number;
    /** Offset within newText just past the inserted control call. */
    caretOffset: number;
    /** The default choice with the cursor on the replaced literal — surfaces
     *  as isPreferred. */
    preferred?: boolean;
}

/** What resolves a call: the module schemas and the DSL lib's index. */
export interface ControlApi {
    schemas: Schemas;
    index: ApiIndex;
}

export function computeControlQuickFixes(
    source: string,
    cursorOffset: number,
    { schemas, index }: ControlApi,
    layout: CodeStyle,
): ControlQuickFix[] {
    const { quote } = layout;
    const file = parseDsl(source);
    const chainAt = (pos: number): ts.Node[] => {
        const chain: ts.Node[] = [];
        for (let n = tokenAt(file, pos); n; n = n.parent) {
            chain.push(n);
        }
        return chain;
    };
    // A caret sits between characters, and tokenAt returns the token
    // starting at an offset, so a caret just past a literal (`2.5|`)
    // resolves to the following token. Literals are searched from the
    // character left of the caret too, so a caret at either edge is on one.
    const atCursor = chainAt(cursorOffset);
    const literalsAtCursor = [
        ...new Set([...chainAt(cursorOffset - 1), ...atCursor]),
    ];
    if (literalsAtCursor.length === 0) {
        return [];
    }
    // tokenAt also resolves trivia positions to the following token.
    // Candidates must contain the cursor by their non-trivia range to count
    // as "at the cursor".
    const containsCursor = (n: ts.Node) =>
        cursorOffset >= n.getStart(file) && cursorOffset <= n.getEnd();

    const resolveCall = createCallResolver(file, schemas, index);
    const existingLabels = collectRootLabels(file);
    const label = (base: string) => dedupeLabel(base, existingLabels);

    /** Fixes replacing `node` as the value of `param` with a control
     *  set to `literal`: the literal itself, or a `$c`/`$signal` holding
     *  just that literal, which the control stands in for. */
    const wrapFixes = (
        node: ts.Node,
        literal: ParamLiteral,
        param: ParamRef,
    ): ControlQuickFix[] => {
        const onLiteral = containsCursor(node);
        const consumed = ts.isCallExpression(node)
            ? `Replace ${shortCallText(node, file)} with `
            : null;
        return wrapChoices(param.label, literal, param.spec, label, quote).map(
            (choice) => ({
                caretOffset: choice.text.length,
                controlOffset: 0,
                kind: 'wrap',
                newText: choice.text,
                preferred: onLiteral && choice.preferred,
                span: { end: node.getEnd(), start: node.getStart(file) },
                title: consumed
                    ? choice.title.replace(WRAP_TITLE_HEAD, consumed)
                    : choice.title,
            }),
        );
    };

    /** The param `expr` sets as a call argument or option member. */
    const paramOf = (expr: ts.Expression): ParamRef | null => {
        const parent = expr.parent;
        if (ts.isCallExpression(parent)) {
            const index = parent.arguments.indexOf(expr);
            return index >= 0
                ? (resolveCall(parent)?.argParam(index) ?? null)
                : null;
        }
        if (ts.isPropertyAssignment(parent) && parent.initializer === expr) {
            const obj = parent.parent;
            const call = obj.parent;
            if (!call || !ts.isCallExpression(call)) {
                return null;
            }
            const key = propertyName(parent);
            const members = resolveCall(call)?.options(
                call.arguments.indexOf(obj),
            );
            return (key && members?.get(key)) || null;
        }
        return null;
    };

    /**
     * Fixes replacing the `$c`/`$signal` call `call`, holding just
     * `literal`, with a control. The control takes the param the call sets
     * when one offers a choice; otherwise the call's own value param, or a
     * generic 0–5 control where that offers none (`$signal` takes audio),
     * named after that param or the variable the call initializes.
     */
    const consumeFixes = (
        call: ts.CallExpression,
        literal: ParamLiteral,
    ): ControlQuickFix[] => {
        const outer = paramOf(call);
        const fromOuter = outer ? wrapFixes(call, literal, outer) : [];
        if (fromOuter.length > 0) {
            return fromOuter;
        }
        const declared =
            ts.isVariableDeclaration(call.parent) &&
            ts.isIdentifier(call.parent.name)
                ? call.parent.name.text
                : null;
        const name = outer?.label ?? declared ?? 'value';
        const own = resolveCall(call)?.argParam(0);
        const fromOwn = own
            ? wrapFixes(call, literal, { ...own, label: name })
            : [];
        return fromOwn.length > 0
            ? fromOwn
            : wrapFixes(call, literal, hintedRef(name, {}));
    };

    const argWraps = ((): ControlQuickFix[] => {
        for (const node of literalsAtCursor) {
            const consumable =
                singleLiteralSignal(node, file) ??
                (node.parent && singleLiteralSignal(node.parent, file));
            if (consumable && containsCursor(consumable.call)) {
                return consumeFixes(consumable.call, consumable.literal);
            }
            const literal = asLiteral(node, file);
            const parent = node.parent;
            if (
                !literal ||
                !containsCursor(node) ||
                !parent ||
                !ts.isCallExpression(parent)
            ) {
                continue;
            }
            const argIndex = parent.arguments.indexOf(node as ts.Expression);
            const resolved = argIndex >= 0 ? resolveCall(parent) : null;
            const param = resolved?.argParam(argIndex);
            if (param) {
                return wrapFixes(node, literal, param);
            }
        }
        return [];
    })();

    const optionWraps = ((): ControlQuickFix[] => {
        for (const node of literalsAtCursor) {
            if (!ts.isPropertyAssignment(node) || !containsCursor(node)) {
                continue;
            }
            const init = node.initializer;
            const consumable = singleLiteralSignal(init, file);
            if (consumable) {
                return consumeFixes(consumable.call, consumable.literal);
            }
            const param = paramOf(init);
            const literal = asLiteral(init, file);
            return param && literal ? wrapFixes(init, literal, param) : [];
        }
        return [];
    })();

    const fixes = [
        ...(argWraps.length > 0 ? argWraps : optionWraps),
        ...unwrapFixes(file, literalsAtCursor, containsCursor),
    ];

    // "Add … for <param>" — signal params the innermost resolvable call
    // doesn't set yet.
    for (const node of atCursor) {
        if (!ts.isCallExpression(node) || !containsCursor(node)) {
            continue;
        }
        const resolved = resolveCall(node);
        if (resolved) {
            fixes.push(
                ...addFixes(file, node, resolved, existingLabels, layout),
            );
            break;
        }
    }

    const errors = parseErrors(file);
    return fixes.filter((fix) => !introducesSyntaxErrors(source, fix, errors));
}

/** The start of a wrap choice's title, before the control it names. */
const WRAP_TITLE_HEAD = /^(?:Wrap '[^']*' in|Replace '[^']*' with) /;

/** Calls that only lift a value into a signal; a control can replace one
 *  holding a single literal outright. */
const SIGNAL_LIFTS = new Set(['$c', '$signal']);

/** `node` as a `$c`/`$signal` call holding just one literal. */
function singleLiteralSignal(
    node: ts.Node,
    file: ts.SourceFile,
): { call: ts.CallExpression; literal: ParamLiteral } | null {
    if (
        !ts.isCallExpression(node) ||
        !ts.isIdentifier(node.expression) ||
        !SIGNAL_LIFTS.has(node.expression.text) ||
        node.arguments.length !== 1
    ) {
        return null;
    }
    const literal = asLiteral(node.arguments[0], file);
    return literal ? { call: node, literal } : null;
}

/**
 * "Replace $slider with $c(…)": the innermost `$slider` call at the cursor
 * becomes a `$c` of its value, as written.
 */
function unwrapFixes(
    file: ts.SourceFile,
    atCursor: readonly ts.Node[],
    containsCursor: (n: ts.Node) => boolean,
): ControlQuickFix[] {
    const slider = atCursor.find(
        (n): n is ts.CallExpression =>
            ts.isCallExpression(n) &&
            ts.isIdentifier(n.expression) &&
            n.expression.text === '$slider' &&
            containsCursor(n),
    );
    const value = slider?.arguments[1];
    if (!slider || !value || !asLiteral(value, file)) {
        return [];
    }
    const newText = `$c(${value.getText(file)})`;
    return [
        {
            caretOffset: newText.length,
            controlOffset: 0,
            kind: 'unwrap',
            newText,
            span: { end: slider.getEnd(), start: slider.getStart(file) },
            title: `Replace $slider with ${newText}`,
        },
    ];
}

/** `call`'s text when short, otherwise its callee and `(…)`. */
function shortCallText(call: ts.CallExpression, file: ts.SourceFile): string {
    const text = call.getText(file);
    return text.length <= 24 && !text.includes('\n')
        ? text
        : `${call.expression.getText(file)}(…)`;
}

/**
 * The deepest node or token whose full range, leading trivia included,
 * holds `pos`: the token starting at `pos`, or the one following trivia
 * there.
 */
function tokenAt(file: ts.SourceFile, pos: number): ts.Node | undefined {
    if (pos < 0) {
        return undefined;
    }
    let node: ts.Node | undefined;
    for (;;) {
        const next: ts.Node | undefined = (node ?? file)
            .getChildren(file)
            .find((c) => c.pos <= pos && pos < c.end);
        if (!next) {
            return node;
        }
        node = next;
    }
}

/**
 * Labels of every ungrouped control in the source. Sliders and buttons share
 * one label namespace per group, and the generated controls are ungrouped.
 */
function collectRootLabels(file: ts.SourceFile): Set<string> {
    const labels = new Set<string>();
    for (const { kind, call, group } of findControlCalls(file)) {
        const first = call.arguments[0];
        if (
            kind !== '$cGroup' &&
            group.length === 0 &&
            first &&
            ts.isStringLiteral(first)
        ) {
            labels.add(first.text);
        }
    }
    return labels;
}
