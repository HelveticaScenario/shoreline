/**
 * Source edits behind the mixer's controls: a track's mute and solo buttons
 * rewrite its label's prefix, its gain fader and pan knob the options of its
 * `.track(...)`, and the master fader the `$setMainGain(...)` level.
 *
 * A track is found at its label's anchor, an offset within the label that
 * the playing patch's label had when it ran, moved along with the user's
 * edits since, and must still carry the track's name there. Each function returns one text edit, or null
 * when the source has no editable form for it; the caller then applies only
 * the live graph update.
 */
import ts from 'typescript';

import { negatableNumber } from '../../shared/dsl/ast/literals';
import { propertyName } from '../../shared/dsl/ast/names';
import { parseDsl } from '../../shared/dsl/ast/parse';
import { forEachDescendant } from '../../shared/dsl/ast/walk';
import {
    parseTrackLabel,
    type TrackState,
    withTrackState,
} from '../../shared/dsl/trackLabels';
import { callParens, closingTokenStart } from './listEdits';

export interface TrackSourceEdit {
    /** Inclusive start character offset */
    start: number;
    /** Exclusive end character offset */
    end: number;
    /** Replacement text */
    text: string;
    /** Whether the edit removes the label's own `.track(...)`, after which
     *  the label's value is no longer a track. */
    dropsTrack?: true;
}

/** Whether `node` is a literal an option's value may be rewritten from. */
export type LiteralCheck = (node: ts.Node) => boolean;

export const isNumber: LiteralCheck = (node) => negatableNumber(node) !== null;

/** The comma tokens on either side of `element` in the argument or
 *  property list holding it. */
export function commasAround(
    element: ts.Node,
    file: ts.SourceFile,
): { before?: ts.Node; after?: ts.Node } {
    const list = element.parent
        .getChildren(file)
        .find(
            (c) =>
                c.kind === ts.SyntaxKind.SyntaxList &&
                c.pos <= element.pos &&
                element.end <= c.end,
        );
    const items = list?.getChildren(file) ?? [];
    const i = items.indexOf(element);
    const comma = (n: ts.Node | undefined) =>
        n?.kind === ts.SyntaxKind.CommaToken ? n : undefined;
    return {
        after: i < 0 ? undefined : comma(items[i + 1]),
        before: i < 0 ? undefined : comma(items[i - 1]),
    };
}

/**
 * The span that removes `element` from its list along with one separator.
 * An element on lines of its own goes with those whole lines, its trailing
 * comment included, so its neighbours keep their comments and layout. An
 * inline element goes with the comma after it, or else the one before it.
 */
export function removalSpan(
    element: ts.Node,
    file: ts.SourceFile,
): { start: number; end: number } {
    const text = file.text;
    const { before, after } = commasAround(element, file);
    const start = element.getStart(file);
    const tail = after ? after.getEnd() : element.getEnd();
    const lineStart = text.lastIndexOf('\n', start - 1) + 1;
    const lineEnd = text.indexOf('\n', tail);
    if (
        lineEnd >= 0 &&
        /^\s*$/.test(text.slice(lineStart, start)) &&
        /^\s*(\/\/[^\n]*)?$/.test(text.slice(tail, lineEnd))
    ) {
        return { end: lineEnd + 1, start: lineStart };
    }
    const blank = (i: number) => text[i] === ' ' || text[i] === '\t';
    if (after) {
        let end = after.getEnd();
        while (blank(end)) {
            end++;
        }
        // Last on its line: take the space before it, not a line break.
        let from = start;
        if (text[end] === '\n' || text[end] === '\r') {
            while (blank(from - 1)) {
                from--;
            }
        }
        return { end, start: from };
    }
    return {
        end: element.getEnd(),
        start: before ? before.getStart(file) : start,
    };
}

/** Whether `node` sits inside a function, where it runs only if called. */
function insideFunction(node: ts.Node): boolean {
    for (let n = node.parent; n; n = n.parent) {
        if (ts.isFunctionLike(n)) {
            return true;
        }
    }
    return false;
}

/** The labelled statement of track `name` whose label starts at `anchor`. */
function trackAt(
    file: ts.SourceFile,
    anchor: number,
    name: string,
): ts.LabeledStatement | undefined {
    // Anywhere in the label: text typed at the label's start, such as a
    // prefix, moves the anchor inside it.
    const statement = file.statements.find(
        (st): st is ts.LabeledStatement =>
            ts.isLabeledStatement(st) &&
            st.label.getStart(file) <= anchor &&
            anchor <= st.label.end,
    );
    if (!statement || !ts.isExpressionStatement(statement.statement)) {
        return undefined;
    }
    const track = parseTrackLabel(statement.label.text);
    return 'error' in track || track.name !== name ? undefined : statement;
}

/** The edit that gives track `name` the label prefix of `state`. */
export function computeTrackStateEdit(
    source: string,
    anchor: number,
    name: string,
    state: TrackState,
): TrackSourceEdit | null {
    const file = parseDsl(source);
    const statement = trackAt(file, anchor, name);
    if (!statement) {
        return null;
    }
    const label = statement.label.text;
    const text = withTrackState(label, state);
    return text === label
        ? null
        : {
              end: statement.label.end,
              start: statement.label.getStart(file),
              text,
          };
}

/** `expression` without the `.scope(...)` calls chained at its end. */
function withoutScopes(expression: ts.Expression): ts.Expression {
    let e = expression;
    while (
        ts.isCallExpression(e) &&
        ts.isPropertyAccessExpression(e.expression) &&
        e.expression.name.text === 'scope'
    ) {
        e = e.expression.expression;
    }
    return e;
}

/** Whether `e` is a `.track(...)` call. */
function isTrackCall(e: ts.Expression): e is ts.CallExpression {
    return (
        ts.isCallExpression(e) &&
        ts.isPropertyAccessExpression(e.expression) &&
        e.expression.name.text === 'track'
    );
}

/** Whether `.track(...)` can chain onto `e` as written, without parentheses. */
function chainable(e: ts.Expression): boolean {
    return (
        ts.isCallExpression(e) ||
        ts.isPropertyAccessExpression(e) ||
        ts.isElementAccessExpression(e) ||
        ts.isIdentifier(e) ||
        ts.isParenthesizedExpression(e) ||
        ts.isTaggedTemplateExpression(e) ||
        ts.isArrayLiteralExpression(e)
    );
}

/** A mixer control's value as the source writes it. */
export function formatControlValue(value: number): string {
    return String(Number(value.toPrecision(4)));
}

/**
 * The edit to the options of track `name`: `inCall` edits the `.track(...)`
 * its label's expression ends with; without one, `appended` is the option
 * text of a `.track(...)` appended to that expression, or null to edit
 * nothing. `fromTrack` says the label's value was a `.track(...)` made
 * elsewhere, such as in a helper, which this edits nothing of.
 */
export function computeTrackOptionsEdit(
    source: string,
    anchor: number,
    name: string,
    fromTrack: boolean,
    inCall: (
        file: ts.SourceFile,
        call: ts.CallExpression,
    ) => TrackSourceEdit | null,
    appended: string | null,
): TrackSourceEdit | null {
    const file = parseDsl(source);
    const statement = trackAt(file, anchor, name);
    if (!statement) {
        return null;
    }
    const expression = (statement.statement as ts.ExpressionStatement)
        .expression;
    const target = withoutScopes(expression);
    if (isTrackCall(target)) {
        return inCall(file, target);
    }
    if (appended === null || fromTrack) {
        return null;
    }
    // After any scopes, which keep drawing the signal they drew.
    const call = `.track({ ${appended} })`;
    if (chainable(expression)) {
        return {
            end: expression.getEnd(),
            start: expression.getEnd(),
            text: call,
        };
    }
    const start = expression.getStart(file);
    return {
        end: expression.getEnd(),
        start,
        text: `(${source.slice(start, expression.getEnd())})${call}`,
    };
}

/**
 * The edit that sets (`value` a number) or removes (`value` null) option
 * `prop` of track `name`, as `computeTrackOptionsEdit` places it.
 */
export function computeTrackOptionEdit(
    source: string,
    anchor: number,
    name: string,
    prop: 'gain' | 'pan',
    value: number | null,
    fromTrack: boolean,
): TrackSourceEdit | null {
    const newText = value === null ? null : formatControlValue(value);
    return computeTrackOptionsEdit(
        source,
        anchor,
        name,
        fromTrack,
        (file, call) => trackOptionEdit(file, call, prop, newText, isNumber),
        newText === null ? null : `${prop}: ${newText}`,
    );
}

/**
 * Compute the text edit that sets the main gain: updates the numeric
 * literal in the last `$setMainGain(...)` outside any function,
 * or appends a new call at the end of the source. A call inside a function
 * is never the one edited: whether and when it runs is the function's
 * caller's business. Returns null when that call's argument is not a
 * numeric literal (signal-driven — the fader is locked), or when it is not a
 * statement of its own at the top of the patch, so whether it runs depends
 * on a branch or loop.
 */
export function computeSetMainGainEdit(
    source: string,
    value: number,
): TrackSourceEdit | null {
    const newText = formatControlValue(value);
    const file = parseDsl(source);
    let last: ts.CallExpression | undefined;
    forEachDescendant(file, (node) => {
        if (
            ts.isCallExpression(node) &&
            ts.isIdentifier(node.expression) &&
            node.expression.text === '$setMainGain' &&
            !insideFunction(node) &&
            (!last || node.getStart(file) > last.getStart(file))
        ) {
            last = node;
        }
    });
    if (last) {
        const arg = last.arguments[0];
        if (
            !ts.isExpressionStatement(last.parent) ||
            last.parent.parent !== file ||
            !callParens(last, file) ||
            last.arguments.length !== 1 ||
            !isNumber(arg)
        ) {
            return null;
        }
        return { end: arg.getEnd(), start: arg.getStart(file), text: newText };
    }
    // No call yet — append one on its own line at the end.
    const needsNewline = source.length > 0 && !source.endsWith('\n');
    return {
        end: source.length,
        start: source.length,
        text: `${needsNewline ? '\n' : ''}$setMainGain(${newText})\n`,
    };
}

/**
 * The edit to option `prop` of the `.track(...)` call `call`: `newText` its
 * new value, or null to remove it. An existing value is rewritten or removed
 * only when `editable` accepts it.
 */
export function trackOptionEdit(
    file: ts.SourceFile,
    call: ts.CallExpression,
    prop: string,
    newText: string | null,
    editable: LiteralCheck,
): TrackSourceEdit | null {
    const source = file.text;
    const bounds = callParens(call, file);
    if (!bounds || call.arguments.length > 1) {
        return null;
    }
    const { open, close } = bounds;
    const optionsArg = call.arguments[0];
    if (optionsArg !== undefined && !ts.isObjectLiteralExpression(optionsArg)) {
        return null;
    }
    const options = optionsArg;

    if (newText !== null) {
        if (!options) {
            return { end: close, start: open, text: `{ ${prop}: ${newText} }` };
        }
        const objClose = closingTokenStart(
            options,
            file,
            ts.SyntaxKind.CloseBraceToken,
        );
        if (objClose === null) {
            return null;
        }
        const existing = findProperty(options, prop);
        if (existing) {
            if (
                !ts.isPropertyAssignment(existing) ||
                !editable(existing.initializer)
            ) {
                return null;
            }
            const init = existing.initializer;
            return {
                end: init.getEnd(),
                start: init.getStart(file),
                text: newText,
            };
        }
        const last = options.properties.at(-1);
        if (!last) {
            return {
                end: objClose + 1,
                start: options.getStart(file),
                text: `{ ${prop}: ${newText} }`,
            };
        }
        // After the last property's own comma, or after the property with a
        // comma of its own; either way ahead of any comment on that line.
        const trailing = commasAround(last, file).after;
        const at = trailing ? trailing.getEnd() : last.getEnd();
        return {
            end: at,
            start: at,
            text: `${trailing ? '' : ','} ${prop}: ${newText}`,
        };
    }

    // --- Remove the property ---
    if (!options) {
        return null;
    }
    const objClose = closingTokenStart(
        options,
        file,
        ts.SyntaxKind.CloseBraceToken,
    );
    const existing = objClose === null ? null : findProperty(options, prop);
    if (
        objClose === null ||
        !existing ||
        !ts.isPropertyAssignment(existing) ||
        !editable(existing.initializer)
    ) {
        return null;
    }
    // Removing one of a repeated key would leave an earlier one in force.
    if (
        options.properties.some(
            (p) => p !== existing && propertyName(p) === prop,
        )
    ) {
        return null;
    }
    const span = removalSpan(existing, file);
    const leftover =
        source.slice(options.getStart(file) + 1, span.start) +
        source.slice(span.end, objClose);
    if (options.properties.length === 1 && leftover.trim() === '') {
        // Nothing left to set: drop the whole `.track(...)`, unless a scope
        // chains on it, which draws the track's mixdown only off a track.
        if (!ts.isPropertyAccessExpression(call.parent)) {
            const receiver = (call.expression as ts.PropertyAccessExpression)
                .expression;
            return {
                dropsTrack: true,
                end: call.getEnd(),
                start: receiver.getEnd(),
                text: '',
            };
        }
        return { end: close, start: open, text: '' };
    }
    return { ...span, text: '' };
}

/** The member of `obj` that sets `prop`. */
export function findProperty(
    obj: ts.ObjectLiteralExpression,
    prop: string,
): ts.ObjectLiteralElementLike | null {
    // The last of a repeated key is the one the object keeps.
    return obj.properties.findLast((p) => propertyName(p) === prop) ?? null;
}
