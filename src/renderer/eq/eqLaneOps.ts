/**
 * The panel's lane-list controls: switch the edited list, add a copy of
 * it (the shared form splits into two), or remove one (a single remaining
 * list becomes the shared form). Like section adds and removes, they change
 * the engine only once the code holds the change.
 */
import { computeEqLaneEdit, type EqLaneOp } from './eqLaneEdit';
import { applyEdit, flushCode, sendEngineNow } from './eqCodeWriter';
import { MAX_EQ_LANE_LISTS } from './eqMath';
import { isPerLane, sectionLists } from './eqPanelModel';
import { editable, markStructure } from './eqPanelOps';
import {
    type CodeGhost,
    type EqPanelContext,
    type PanelRuntime,
    parseSectionKey,
    sectionKey,
} from './eqPanelRuntime';

export function selectLane(
    ctx: EqPanelContext,
    panel: PanelRuntime,
    list: number,
): void {
    if (list === panel.lane || list >= sectionLists(panel.params).length) {
        return;
    }
    flushCode(ctx, panel, true);
    panel.lane = list;
    panel.selected = null;
    panel.spectrum.reset();
    ctx.render(panel);
}

function applyLaneOp(
    ctx: EqPanelContext,
    panel: PanelRuntime,
    listCount: number,
    op: EqLaneOp,
): boolean {
    return applyEdit(ctx, panel, (source, anchor, layout) =>
        computeEqLaneEdit(source, anchor, listCount, op, layout),
    );
}

/**
 * Add a lane list copying the edited one; the shared form splits into
 * two copies. Code-only values travel with the copy, since the code copy
 * carries them.
 */
export function addLane(ctx: EqPanelContext, panel: PanelRuntime): void {
    const lists = sectionLists(panel.params);
    const perLane = isPerLane(panel.params);
    if (lists.length >= MAX_EQ_LANE_LISTS) {
        return;
    }
    flushCode(ctx, panel, false);
    const from = perLane ? panel.lane : 0;
    if (
        editable(ctx, panel) &&
        !applyLaneOp(
            ctx,
            panel,
            perLane ? lists.length : 0,
            perLane ? { from, kind: 'addLane' } : { kind: 'split' },
        )
    ) {
        ctx.render(panel);
        return;
    }
    const copy = structuredClone(lists[from]);
    panel.params.sections = perLane ? [...lists, copy] : [lists[0], copy];
    const added = sectionLists(panel.params).length - 1;
    for (const [key, g] of [...panel.ghosts]) {
        const at = parseSectionKey(key);
        if (at.list === from) {
            panel.ghosts.set(sectionKey(added, at.index), { ...g });
        }
    }
    panel.live = null;
    markStructure(ctx, panel);
    selectLane(ctx, panel, added);
    sendEngineNow(ctx, panel);
    ctx.render(panel);
}

/** Remove lane list `list`; a single remaining list becomes shared. */
export function removeLane(
    ctx: EqPanelContext,
    panel: PanelRuntime,
    list: number,
): void {
    const lists = sectionLists(panel.params);
    if (!isPerLane(panel.params) || list < 0 || list >= lists.length) {
        return;
    }
    flushCode(ctx, panel, false);
    if (
        editable(ctx, panel) &&
        !applyLaneOp(ctx, panel, lists.length, {
            index: list,
            kind: 'removeLane',
        })
    ) {
        ctx.render(panel);
        return;
    }
    const rest = lists.filter((_, l) => l !== list);
    panel.params.sections = rest.length === 1 ? rest[0] : rest;
    const ghosts = new Map<string, CodeGhost>();
    for (const [key, g] of panel.ghosts) {
        const at = parseSectionKey(key);
        if (at.list !== list) {
            ghosts.set(
                sectionKey(at.list > list ? at.list - 1 : at.list, at.index),
                g,
            );
        }
    }
    panel.ghosts = ghosts;
    panel.live = null;
    panel.lane = Math.min(
        panel.lane > list ? panel.lane - 1 : panel.lane,
        rest.length - 1,
    );
    panel.selected = null;
    panel.spectrum.reset();
    markStructure(ctx, panel);
    sendEngineNow(ctx, panel);
    ctx.render(panel);
}
