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

import { findControlCalls } from '../../shared/dsl/controlCalls';
import type { Schemas } from '../../shared/dsl/schemaTypeResolver';
import type { ParamRef } from './apiParamControls';
import type { ApiIndex } from './apiSignatures';
import {
    appendArgument,
    appendObjectArgument,
    appendObjectProperty,
} from './listEdits';
import type { CodeStyle, ListEdit, TextEdit } from './listEdits';
import { introducesSyntaxErrors, parseErrors } from './syntaxCheck';
import { asLiteral, createCallResolver } from './paramCallResolver';
import type { ResolvedCall } from './paramCallResolver';
import { addChoices, wrapChoices } from './paramControls';
import type { ParamLiteral } from './paramControls';

export interface ControlQuickFix extends TextEdit {
    title: string;
    kind: 'wrap' | 'add';
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
    const file = ts.createSourceFile(
        'quickfix.js',
        source,
        ts.ScriptTarget.Latest,
        true,
        ts.ScriptKind.JS,
    );
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
            const key = propertyKeyName(node, file);
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
            fixes.push(...addFixes(file, node, resolved, label, layout));
            break;
        }
    }

    const errors = parseErrors(file);
    return fixes.filter((fix) => !introducesSyntaxErrors(source, fix, errors));
}

/**
 * The "add" actions for one resolved call: every unset signal param with a
 * known default that can be set without disturbing the call's positional
 * shape — through its option object, or as the very next argument.
 */
function addFixes(
    file: ts.SourceFile,
    call: ts.CallExpression,
    resolved: ResolvedCall,
    label: (base: string) => string,
    layout: CodeStyle,
): ControlQuickFix[] {
    const { quote } = layout;
    const args = call.arguments;
    const fixes: ControlQuickFix[] = [];
    /** `edit` appends an element whose control call `control` starts
     *  `controlAt` into it. */
    const push = (
        title: string,
        control: string,
        edit: ListEdit | null,
        controlAt: number,
    ) => {
        if (edit) {
            const start = edit.elementOffset + controlAt;
            fixes.push({
                caretOffset: start + control.length,
                controlOffset: start,
                kind: 'add',
                newText: edit.newText,
                span: edit.span,
                title,
            });
        }
    };

    // Option members: into the option object the call already passes, or
    // as a new trailing object argument. An option argument that is some
    // other expression (a variable, a call) may already set any member, so
    // none is offered.
    const optionsIndex = args.findIndex(
        (a, i) => ts.isObjectLiteralExpression(a) && resolved.options(i),
    );
    const found = optionsIndex >= 0 ? args[optionsIndex] : undefined;
    const obj = found && ts.isObjectLiteralExpression(found) ? found : null;
    const opaque = args.some(
        (a, i) =>
            resolved.options(i) &&
            !ts.isObjectLiteralExpression(a) &&
            !asLiteral(a, file) &&
            !isKeywordLiteral(a),
    );
    const members = opaque
        ? null
        : resolved.options(obj ? optionsIndex : args.length);
    if (members) {
        const supplied = new Set(resolved.positionalNames(args.length));
        for (const prop of obj?.properties ?? []) {
            const key = propertyKeyName(prop, file);
            if (key) {
                supplied.add(key);
            }
        }
        for (const [key, param] of members) {
            if (!param?.knownDefault || supplied.has(key)) {
                continue;
            }
            const prefix = `${propertyKey(key, quote)}: `;
            for (const choice of addChoices(
                param.label,
                param.spec,
                label,
                quote,
            )) {
                const prop = prefix + choice.text;
                const edit = obj
                    ? appendObjectProperty(file, obj, prop, layout)
                    : appendObjectArgument(file, call, prop, layout);
                push(choice.title, choice.text, edit, prefix.length);
            }
        }
    }

    // The next positional argument, when no option object comes first.
    if (!obj) {
        const next = resolved.argParam(args.length);
        if (next?.knownDefault && !resolved.options(args.length)) {
            for (const choice of addChoices(
                next.label,
                next.spec,
                label,
                quote,
            )) {
                const edit = appendArgument(file, call, choice.text);
                push(choice.title, choice.text, edit, 0);
            }
        }
    }
    return fixes;
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

function isKeywordLiteral(node: ts.Node): boolean {
    return (
        node.kind === ts.SyntaxKind.TrueKeyword ||
        node.kind === ts.SyntaxKind.FalseKeyword ||
        node.kind === ts.SyntaxKind.NullKeyword
    );
}

function propertyKey(key: string, quote: string): string {
    return /^[A-Za-z_$][\w$]*$/.test(key) ? key : `${quote}${key}${quote}`;
}

function propertyKeyName(node: ts.Node, file: ts.SourceFile): string | null {
    if (ts.isShorthandPropertyAssignment(node)) {
        return node.name.text;
    }
    if (!ts.isPropertyAssignment(node)) {
        return null;
    }
    const name = node.name;
    if (ts.isStringLiteral(name) || ts.isNoSubstitutionTemplateLiteral(name)) {
        return name.text;
    }
    return name.getText(file);
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

function dedupeLabel(base: string, taken: Set<string>): string {
    if (!taken.has(base)) {
        return base;
    }
    for (let n = 2; ; n++) {
        const candidate = `${base} ${n}`;
        if (!taken.has(candidate)) {
            return candidate;
        }
    }
}
