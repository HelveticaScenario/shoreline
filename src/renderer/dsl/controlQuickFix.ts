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
    kind: 'wrap' | 'add' | 'addAll';
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

    /** Fixes replacing the literal `node` as the value of `param`. */
    const wrapFixes = (
        node: ts.Node,
        literal: ParamLiteral,
        param: ParamRef,
    ): ControlQuickFix[] => {
        const onLiteral = containsCursor(node);
        return wrapChoices(param.label, literal, param.spec, label, quote).map(
            (choice) => ({
                caretOffset: choice.text.length,
                controlOffset: 0,
                kind: 'wrap',
                newText: choice.text,
                preferred: onLiteral && choice.preferred,
                span: { end: node.getEnd(), start: node.getStart(file) },
                title: choice.title,
            }),
        );
    };

    const argWraps = ((): ControlQuickFix[] => {
        for (const node of literalsAtCursor) {
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
            const obj = node.parent;
            const call = obj.parent;
            if (!call || !ts.isCallExpression(call)) {
                continue;
            }
            const members = resolveCall(call)?.options(
                call.arguments.indexOf(obj),
            );
            if (!members) {
                continue;
            }
            const key = propertyName(node);
            const param = key ? members.get(key) : null;
            const init = node.initializer;
            const literal = asLiteral(init, file);
            if (param && literal) {
                return wrapFixes(init, literal, param);
            }
            return [];
        }
        return [];
    })();

    const fixes = argWraps.length > 0 ? argWraps : optionWraps;

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
