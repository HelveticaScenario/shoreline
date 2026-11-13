/**
 * Source edits for the panel's lane-list controls: splitting the shared
 * list into per-lane copies, adding a copy of a lane's list, and
 * removing one (two remaining lists collapse back to the shared form).
 */
import ts from 'typescript';

import {
    type CodeStyle,
    eolOf,
    fitsOnLine,
    indentAt,
    indentUnit,
    appendArrayElement,
    appendObjectProperty,
} from '../dsl/listEdits';
import {
    type EqSourceEditResult,
    appendResult,
    isPerLaneLiteral,
    locateSections,
    removeElement,
} from './eqSourceLocate';

/** A change to the lane lists themselves. */
export type EqLaneOp =
    /** Shared list → two per-lane copies of it. */
    | { kind: 'split' }
    /** Append a copy of lane list `from`. */
    | { kind: 'addLane'; from: number }
    /** Remove lane list `index`; one remaining list becomes the shared form. */
    | { kind: 'removeLane'; index: number };

/** Shift every line after the first by `delta` indent units (±1). */
function reindent(text: string, unit: string, delta: 1 | -1): string {
    if (delta === 1) {
        return text.replace(/\n/g, `\n${unit}`);
    }
    const escaped = unit.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
    return text.replace(new RegExp(`\n${escaped}`, 'g'), '\n');
}

/**
 * Compute the edit for a lane-list op. `listCount` is the number of
 * lane lists the panel holds, 0 for the shared form.
 */
export function computeEqLaneEdit(
    source: string,
    anchor: number,
    listCount: number,
    op: EqLaneOp,
    layout: CodeStyle,
): EqSourceEditResult {
    const found = locateSections(source, anchor);
    if (!found.ok) {
        return found;
    }
    const { config, outer, sf } = found;
    const perLane = outer !== null && isPerLaneLiteral(outer);
    const unit = indentUnit(layout);
    const nl = eolOf(source);

    if (op.kind === 'split') {
        if (perLane || listCount !== 0) {
            return { ok: false, reason: 'stale' };
        }
        if (!outer) {
            return appendResult(
                appendObjectProperty(sf, config, 'sections: [[], []]', layout),
            );
        }
        const start = outer.getStart(sf);
        const list = source.slice(start, outer.getEnd());
        const span = { end: outer.getEnd(), start };
        const inline = `[${list}, ${list}]`;
        if (
            !list.includes('\n') &&
            fitsOnLine(source, { newText: inline, span }, layout)
        ) {
            return { edit: { ...span, text: inline }, ok: true, skipped: [] };
        }
        const base = indentAt(source, start);
        const copy = `${base}${unit}${reindent(list, unit, 1)},`;
        return {
            edit: { ...span, text: `[${nl}${copy}${nl}${copy}${nl}${base}]` },
            ok: true,
            skipped: [],
        };
    }

    if (!outer || !perLane || outer.elements.length !== listCount) {
        return { ok: false, reason: 'stale' };
    }
    if (op.kind === 'addLane') {
        const from = outer.elements[op.from];
        if (!from || !ts.isArrayLiteralExpression(from)) {
            return { ok: false, reason: 'unsupported' };
        }
        return appendResult(appendArrayElement(sf, outer, from.getText(sf)));
    }
    if (listCount === 2) {
        const keep = outer.elements[op.index === 0 ? 1 : 0];
        return {
            edit: {
                end: outer.getEnd(),
                start: outer.getStart(sf),
                text: reindent(keep.getText(sf), unit, -1),
            },
            ok: true,
            skipped: [],
        };
    }
    return {
        edit: removeElement(sf, outer, op.index),
        ok: true,
        skipped: [],
    };
}
