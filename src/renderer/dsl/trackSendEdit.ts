/**
 * Source edits behind the mixer's send dials: each sets, adds or removes one
 * bus's entry in a track's `sends` option, in the forms the option takes: a
 * bus's name (full level), a `[name, level]` pair, or an array of them.
 */
import ts from 'typescript';

import { closingTokenStart } from './listEdits';
import {
    commasAround,
    computeTrackOptionsEdit,
    findProperty,
    formatControlValue,
    isNumber,
    removalSpan,
    trackOptionEdit,
    type TrackSourceEdit,
} from './trackSourceEdit';

/** The level a send naming its bus alone plays at. */
const FULL_LEVEL = 5;

/** One send as written: its node, its bus, and its level when a pair. */
interface SendEntry {
    node: ts.Expression;
    bus: string;
    level?: ts.Expression;
}

/** Whether `node` names one of `buses`. */
function busName(node: ts.Node | undefined, buses: ReadonlySet<string>) {
    return node !== undefined && ts.isIdentifier(node) && buses.has(node.text)
        ? node.text
        : null;
}

/**
 * Whether `node` is one `[bus, level]` pair rather than a list of sends. The
 * DSL tells them apart by what the second element holds when it runs, so a
 * pair is recognised only where the text alone settles it: a level that is
 * no name, spread or array, any of which may hold another send.
 */
function isPair(
    node: ts.Expression,
    buses: ReadonlySet<string>,
): node is ts.ArrayLiteralExpression {
    if (!ts.isArrayLiteralExpression(node) || node.elements.length !== 2) {
        return false;
    }
    const [bus, level] = node.elements;
    return (
        busName(bus, buses) !== null &&
        !ts.isIdentifier(level) &&
        !ts.isSpreadElement(level) &&
        !ts.isOmittedExpression(level) &&
        !ts.isArrayLiteralExpression(level)
    );
}

/** One element of a `sends` value as an entry, or null when it is none. */
function entryOf(
    node: ts.Expression,
    buses: ReadonlySet<string>,
): SendEntry | null {
    const bus = busName(node, buses);
    if (bus !== null) {
        return { bus, node };
    }
    if (isPair(node, buses)) {
        return {
            bus: busName(node.elements[0], buses)!,
            level: node.elements[1],
            node,
        };
    }
    return null;
}

/**
 * The entries of a `sends` value, and the array listing them when it is a
 * list; null when any part of it is not written in a form the dials edit.
 */
function parseSends(
    value: ts.Expression,
    buses: ReadonlySet<string>,
): { entries: SendEntry[]; list: ts.ArrayLiteralExpression | null } | null {
    const single = entryOf(value, buses);
    if (single) {
        return { entries: [single], list: null };
    }
    if (!ts.isArrayLiteralExpression(value)) {
        return null;
    }
    const entries: SendEntry[] = [];
    for (const element of value.elements) {
        const entry = entryOf(element, buses);
        if (!entry) {
            return null;
        }
        entries.push(entry);
    }
    return { entries, list: value };
}

/** The `.track(...)` call's options object, when it is one the dials edit. */
function optionsOf(
    call: ts.CallExpression,
): ts.ObjectLiteralExpression | undefined | null {
    if (call.arguments.length > 1) {
        return null;
    }
    const options = call.arguments[0];
    if (options === undefined) {
        return undefined;
    }
    return ts.isObjectLiteralExpression(options) ? options : null;
}

/** The edit to the sends of the `.track(...)` call `call`. */
function sendsEdit(
    file: ts.SourceFile,
    call: ts.CallExpression,
    bus: string,
    levelText: string | null,
    buses: ReadonlySet<string>,
): TrackSourceEdit | null {
    const options = optionsOf(call);
    // A spread may hold sends of its own, which an edit here would override.
    if (options === null || options?.properties.some(ts.isSpreadAssignment)) {
        return null;
    }
    const property = options && findProperty(options, 'sends');
    if (!property) {
        return levelText === null
            ? null
            : trackOptionEdit(
                  file,
                  call,
                  'sends',
                  `[${bus}, ${levelText}]`,
                  () => false,
              );
    }
    if (!ts.isPropertyAssignment(property)) {
        return null;
    }
    const sends = parseSends(property.initializer, buses);
    if (!sends) {
        return null;
    }
    const matches = sends.entries.filter((e) => e.bus === bus);
    if (matches.length > 1) {
        return null;
    }
    const [entry] = matches;
    const replace = (node: ts.Node, text: string): TrackSourceEdit | null =>
        node.getText(file) === text
            ? null
            : { end: node.getEnd(), start: node.getStart(file), text };

    if (levelText === null) {
        if (!entry) {
            return null;
        }
        if (!sends.list || sends.entries.length === 1) {
            return trackOptionEdit(file, call, 'sends', null, () => true);
        }
        return { ...removalSpan(entry.node, file), text: '' };
    }

    if (entry) {
        if (entry.level) {
            return isNumber(entry.level)
                ? replace(entry.level, levelText)
                : null;
        }
        return Number(levelText) === FULL_LEVEL
            ? null
            : replace(entry.node, `[${bus}, ${levelText}]`);
    }
    const added = `[${bus}, ${levelText}]`;
    if (!sends.list) {
        const value = property.initializer;
        return replace(value, `[${value.getText(file)}, ${added}]`);
    }
    const last = sends.list.elements.at(-1);
    if (!last) {
        const close = closingTokenStart(
            sends.list,
            file,
            ts.SyntaxKind.CloseBracketToken,
        );
        return close === null
            ? null
            : { end: close, start: close, text: added };
    }
    const trailing = commasAround(last, file).after;
    const at = trailing ? trailing.getEnd() : last.getEnd();
    return { end: at, start: at, text: `${trailing ? '' : ','} ${added}` };
}

/**
 * The edit that sets (`level` a number) or removes (`level` null) track
 * `name`'s send to `bus`. A send added to a track is written as a
 * `[bus, level]` pair; one written as the bus's name alone stays so at full
 * level. Removing a track's last send removes its `sends` option, and its
 * `.track(...)` with it when no other option is left. `buses` names every
 * bus, which tells a `[bus, level]` pair from a list of two sends.
 */
export function computeTrackSendEdit(
    source: string,
    anchor: number,
    name: string,
    bus: string,
    level: number | null,
    fromTrack: boolean,
    buses: ReadonlySet<string>,
): TrackSourceEdit | null {
    const levelText = level === null ? null : formatControlValue(level);
    return computeTrackOptionsEdit(
        source,
        anchor,
        name,
        fromTrack,
        (file, call) => sendsEdit(file, call, bus, levelText, buses),
        levelText === null ? null : `sends: [${bus}, ${levelText}]`,
    );
}
