/**
 * The "add" control quick-fixes for one resolved call: a control for each
 * signal param the call doesn't set yet, and one action adding a control
 * for every such param at once.
 */
import ts from 'typescript';

import { propertyName } from '../../shared/dsl/ast/names';
import type { ControlQuickFix } from './controlQuickFix';
import {
    appendArgument,
    appendObjectArgument,
    appendObjectProperty,
} from './listEdits';
import type { CodeStyle, ListEdit } from './listEdits';
import { asLiteral } from './paramCallResolver';
import type { ResolvedCall } from './paramCallResolver';
import type { ParamRef } from './apiParamControls';
import { addAllChoice, addChoices } from './paramControls';

export const ADD_ALL_TITLE = 'Add controls for all unset params';

/** Where a call can take new params without disturbing its positional
 *  shape. */
interface AddSlots {
    /** The option object the call already passes. */
    obj: ts.ObjectLiteralExpression | null;
    /** Unset option members with a known default, by key. */
    members: [string, ParamRef][];
}

/**
 * The "add" actions for one resolved call: every unset signal param with a
 * known default that can be set without disturbing the call's positional
 * shape — through its option object, or as the very next argument — then,
 * when more than one param can be added, the action adding them all.
 * `taken` holds the labels already in use.
 */
export function addFixes(
    file: ts.SourceFile,
    call: ts.CallExpression,
    resolved: ResolvedCall,
    taken: ReadonlySet<string>,
    layout: CodeStyle,
): ControlQuickFix[] {
    const { quote } = layout;
    const args = call.arguments;
    const slots = addSlots(file, call, resolved);
    const label = (base: string) => dedupeLabel(base, taken);
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
            fixes.push(addFix('add', title, edit, controlAt, control.length));
        }
    };

    for (const [key, param] of slots.members) {
        const prefix = `${propertyKey(key, quote)}: `;
        for (const choice of addChoices(
            param.label,
            param.spec,
            label,
            quote,
        )) {
            const prop = prefix + choice.text;
            const edit = slots.obj
                ? appendObjectProperty(file, slots.obj, prop, layout)
                : appendObjectArgument(file, call, prop, layout);
            push(choice.title, choice.text, edit, prefix.length);
        }
    }

    if (!slots.obj) {
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

    const all = addAllFix(file, call, resolved, slots, taken, layout);
    if (all) {
        fixes.push(all);
    }
    return fixes;
}

/**
 * Every unset param at once, each with its `addAllChoice` control: the run
 * of open positional slots, then the option members those leave unset. Null
 * when fewer than two params can be added.
 */
function addAllFix(
    file: ts.SourceFile,
    call: ts.CallExpression,
    resolved: ResolvedCall,
    slots: AddSlots,
    taken: ReadonlySet<string>,
    layout: CodeStyle,
): ControlQuickFix | null {
    const { quote } = layout;
    const used = new Set(taken);
    const label = (base: string) => {
        const unique = dedupeLabel(base, used);
        used.add(unique);
        return unique;
    };

    const positionals: string[] = [];
    let argCount = call.arguments.length;
    let members = slots.members;
    if (!slots.obj) {
        // A rest param resolves at every index, so the run stops at a param
        // it has already filled.
        const filled = new Set<string>();
        for (;;) {
            const param = resolved.argParam(argCount);
            if (
                !param?.knownDefault ||
                resolved.options(argCount) ||
                filled.has(param.label)
            ) {
                break;
            }
            const choice = addAllChoice(param.label, param.spec, label, quote);
            if (!choice) {
                break;
            }
            filled.add(param.label);
            positionals.push(choice.text);
            argCount++;
        }
        const set = resolved.positionalNames(argCount);
        members =
            argCount === call.arguments.length
                ? members
                : optionMembers(resolved, argCount, null).filter(
                      ([key]) => !set.has(key),
                  );
    }

    const props: string[] = [];
    let lastControl = '';
    for (const [key, param] of members) {
        const choice = addAllChoice(param.label, param.spec, label, quote);
        if (choice) {
            props.push(`${propertyKey(key, quote)}: ${choice.text}`);
            lastControl = choice.text;
        }
    }
    if (props.length === 0 && positionals.length > 0) {
        lastControl = positionals[positionals.length - 1];
    }
    if (positionals.length + props.length < 2) {
        return null;
    }

    // The new elements go in as one; formatting the fix lays them out. The
    // last control ends `controlsEnd` into the element.
    const joinedProps = props.join(', ');
    let edit: ListEdit | null;
    let controlsEnd: number;
    if (slots.obj) {
        edit = appendObjectProperty(file, slots.obj, joinedProps, layout);
        controlsEnd = joinedProps.length;
    } else {
        const pad = layout.bracketSpacing ? ' ' : '';
        const elements =
            props.length > 0
                ? [...positionals, `{${pad}${joinedProps}${pad}}`]
                : positionals;
        const text = elements.join(', ');
        edit = appendArgument(file, call, text);
        controlsEnd =
            props.length > 0 ? text.length - 1 - pad.length : text.length;
    }
    return (
        edit &&
        addFix(
            'addAll',
            ADD_ALL_TITLE,
            edit,
            controlsEnd - lastControl.length,
            lastControl.length,
        )
    );
}

function addFix(
    kind: 'add' | 'addAll',
    title: string,
    edit: ListEdit,
    controlAt: number,
    controlLength: number,
): ControlQuickFix {
    const start = edit.elementOffset + controlAt;
    return {
        caretOffset: start + controlLength,
        controlOffset: start,
        kind,
        newText: edit.newText,
        span: edit.span,
        title,
    };
}

/**
 * The call's option object and its unset members. An option argument that
 * is some other expression (a variable, a call) may already set any member,
 * so none is offered.
 */
function addSlots(
    file: ts.SourceFile,
    call: ts.CallExpression,
    resolved: ResolvedCall,
): AddSlots {
    const args = call.arguments;
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
    if (opaque) {
        return { members: [], obj };
    }
    const supplied = new Set(resolved.positionalNames(args.length));
    return {
        members: optionMembers(
            resolved,
            obj ? optionsIndex : args.length,
            obj,
        ).filter(([key]) => !supplied.has(key)),
        obj,
    };
}

/** The option members argument `index` takes that `obj` doesn't set and a
 *  control can add at a known default. */
function optionMembers(
    resolved: ResolvedCall,
    index: number,
    obj: ts.ObjectLiteralExpression | null,
): [string, ParamRef][] {
    const members = resolved.options(index);
    if (!members) {
        return [];
    }
    const set = new Set<string>();
    for (const prop of obj?.properties ?? []) {
        const key = optionKey(prop);
        if (key) {
            set.add(key);
        }
    }
    const open: [string, ParamRef][] = [];
    for (const [key, param] of members) {
        if (param?.knownDefault && !set.has(key)) {
            open.push([key, param]);
        }
    }
    return open;
}

export function dedupeLabel(base: string, taken: ReadonlySet<string>): string {
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

/** The key an option object member sets, or null for a method, accessor,
 *  spread, or computed key. */
function optionKey(p: ts.ObjectLiteralElementLike): string | null {
    return ts.isPropertyAssignment(p) || ts.isShorthandPropertyAssignment(p)
        ? propertyName(p)
        : null;
}
