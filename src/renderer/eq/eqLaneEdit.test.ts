import { describe, expect, it } from 'vitest';

import type { CodeStyle } from '../dsl/listEdits';
import { computeEqLaneEdit, type EqLaneOp } from './eqLaneEdit';

const layout: CodeStyle = {
    bracketSpacing: true,
    printWidth: 80,
    quote: "'",
    tabWidth: 4,
    trailingComma: true,
    useTabs: false,
};

/** The panel's anchor: the `$eq` callee, or the `eq` of a `.eq(` chain. */
const anchorOf = (source: string) =>
    source.includes('.eq(')
        ? source.lastIndexOf('eq(')
        : source.lastIndexOf('$eq(');

const multi = `const x = $eq($saw('c2'), {
    sections: [
        { type: 'lowShelf', freq: "150hz", gain: 3 },
        { freq: '1000hz', gain: -6, q: 2 }, // mids
    ],
    ui: true,
}).out();
`;

describe('computeEqLaneEdit', () => {
    const lane = (source: string, listCount: number, op: EqLaneOp): string => {
        const res = computeEqLaneEdit(
            source,
            anchorOf(source),
            listCount,
            op,
            layout,
        );
        if (!res.ok) {
            throw new Error(res.reason);
        }
        return res.edit
            ? source.slice(0, res.edit.start) +
                  res.edit.text +
                  source.slice(res.edit.end)
            : source;
    };

    it('splits a one-line shared list into two copies inline', () => {
        expect(
            lane(`$eq(x, { sections: [{ freq: 0 }], ui: true })`, 0, {
                kind: 'split',
            }),
        ).toBe(
            `$eq(x, { sections: [[{ freq: 0 }], [{ freq: 0 }]], ui: true })`,
        );
    });

    it('splits a multi-line shared list one copy per line', () => {
        expect(lane(multi, 0, { kind: 'split' })).toContain(
            `    sections: [
        [
            { type: 'lowShelf', freq: "150hz", gain: 3 },
            { freq: '1000hz', gain: -6, q: 2 }, // mids
        ],
        [
            { type: 'lowShelf', freq: "150hz", gain: 3 },
            { freq: '1000hz', gain: -6, q: 2 }, // mids
        ],
    ],`,
        );
    });

    it('splits a config without sections into two empty lists', () => {
        expect(lane(`$eq(x, { ui: true })`, 0, { kind: 'split' })).toBe(
            `$eq(x, { ui: true, sections: [[], []] })`,
        );
    });

    it('adds a copy of a lane list', () => {
        expect(
            lane(
                `$eq(x, { sections: [[{ freq: 0 }], [{ freq: 1 }]], ui: true })`,
                2,
                { from: 1, kind: 'addLane' },
            ),
        ).toBe(
            `$eq(x, { sections: [[{ freq: 0 }], [{ freq: 1 }], [{ freq: 1 }]], ui: true })`,
        );
    });

    it('removes a lane list, collapsing the last two to the shared form', () => {
        const three = `$eq(x, { sections: [[{ freq: 0 }], [{ freq: 1 }], [{ freq: 2 }]], ui: true })`;
        expect(lane(three, 3, { index: 1, kind: 'removeLane' })).toBe(
            `$eq(x, { sections: [[{ freq: 0 }], [{ freq: 2 }]], ui: true })`,
        );
        const two = `$eq(x, {
    sections: [
        [
            { freq: 0 },
            { freq: 1 },
        ],
        [{ freq: 2 }],
    ],
    ui: true,
})`;
        expect(lane(two, 2, { index: 1, kind: 'removeLane' })).toBe(
            `$eq(x, {
    sections: [
        { freq: 0 },
        { freq: 1 },
    ],
    ui: true,
})`,
        );
    });

    it('reports a mismatched form or lane count as stale', () => {
        const per = `$eq(x, { sections: [[{ freq: 0 }], [{ freq: 1 }]], ui: true })`;
        const anchor = anchorOf(per);
        expect(
            computeEqLaneEdit(per, anchor, 0, { kind: 'split' }, layout),
        ).toEqual({ ok: false, reason: 'stale' });
        expect(
            computeEqLaneEdit(
                per,
                anchor,
                3,
                { index: 0, kind: 'removeLane' },
                layout,
            ),
        ).toEqual({ ok: false, reason: 'stale' });
    });
});
