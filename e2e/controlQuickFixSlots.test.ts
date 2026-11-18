/**
 * Control quick-fixes on literals inside arguments — poly array elements,
 * list and tuple items, nested option members — and on required API
 * signals, applied in the running editor. Each fixed patch must still run.
 */

import type { Page } from '@playwright/test';
import { test, expect, openUntitledBuffer } from './fixtures';

const ROWS = '.action-widget .monaco-list-row';

/** Put `marked` in the patch editor with the caret at the `|` marker. */
async function setCode(page: Page, marked: string) {
    await page.keyboard.press('Escape');
    await page.evaluate(
        ({ code, column }) => {
            const ed = window.monaco.editor
                .getEditors()
                .find((e) => e.getModel()?.getLanguageId() === 'javascript')!;
            ed.setValue(code);
            ed.setPosition({ column, lineNumber: 1 });
            ed.focus();
        },
        { code: marked.replace('|', ''), column: marked.indexOf('|') + 1 },
    );
}

function triggerQuickFix(page: Page) {
    return page.evaluate(() => {
        window.monaco.editor
            .getEditors()
            .find((e) => e.getModel()?.getLanguageId() === 'javascript')!
            .trigger('e2e', 'editor.action.quickFix', {});
    });
}

/** `code` with every quote as `'` and no whitespace or trailing commas,
 *  so sources compare however the formatter laid them out. */
function flatten(code: string): string {
    return code
        .replace(/[`"]/g, "'")
        .replace(/\s+/g, '')
        .replace(/,([\])}])/g, '$1');
}

function editorValue(page: Page): Promise<string> {
    return page
        .evaluate(() =>
            window.monaco.editor
                .getEditors()
                .find((e) => e.getModel()?.getLanguageId() === 'javascript')!
                .getValue(),
        )
        .then(flatten);
}

/** Applies the quick fix titled `title` at the marker and returns the
 *  edited source. */
async function applyFix(
    page: Page,
    marked: string,
    title: string,
): Promise<string> {
    await setCode(page, marked);
    const before = await editorValue(page);
    await triggerQuickFix(page);
    const rows = page.locator(ROWS);
    await expect(rows.first()).toBeVisible({ timeout: 20_000 });
    const titles = await rows.allInnerTexts();
    const at = titles.findIndex((t) => t.includes(title));
    expect(at, titles.join('\n')).toBeGreaterThanOrEqual(0);
    // The widget's pointer-block overlay swallows real clicks.
    await rows.nth(at).dispatchEvent('click');
    await expect.poll(() => editorValue(page)).not.toBe(before);
    return editorValue(page);
}

/** Runs the editor's patch and returns its errors, none when it ran. */
async function runPatch(page: Page): Promise<string[]> {
    await page.evaluate(() => {
        const w = window as unknown as { __prevResult?: unknown };
        w.__prevResult = window.__TEST_API__!.getLastPatchResult();
        void window.__TEST_API__!.executePatch();
    });
    await expect
        .poll(
            () =>
                page.evaluate(
                    () =>
                        window.__TEST_API__!.getLastPatchResult() !==
                        (window as unknown as { __prevResult?: unknown })
                            .__prevResult,
                ),
            { timeout: 20_000 },
        )
        .toBe(true);
    return page.evaluate(() => {
        const result = window.__TEST_API__!.getLastPatchResult() as {
            success: boolean;
            errors?: { message: string }[];
        };
        return result.success
            ? []
            : (result.errors ?? []).map((e) => e.message);
    });
}

const RUNNABLE: {
    name: string;
    marked: string;
    title: string;
    want: string;
}[] = [
    {
        marked: `out: $saw(['a3', 'c|4'])`,
        name: 'a poly pitch element',
        title: `Wrap 'freq' in a note $slider`,
        want: `out: $saw(['a3', $slider('freq', 'c4', 'c2', 'c6')])`,
    },
    {
        marked: `out: $saw('a3').lpf(['200hz', '4|00hz'])`,
        name: 'a poly chain element',
        title: `Wrap 'cutoff' in an hz $slider`,
        want: `$slider('cutoff', '400hz'`,
    },
    {
        marked: `out: $saw('a3', { shape: [1, 2.|5] })`,
        name: 'a poly option element',
        title: `Wrap 'shape' in $slider`,
        want: `{ shape: [1, $slider('shape', 2.5, 0, 5)] }`,
    },
    {
        marked: `out: $pWarp($saw('1hz').range(0, 1), $phase.bend([1.|5, 2]))`,
        name: 'a poly element of a $phase amount',
        title: `Wrap 'amount' in $slider`,
        want: `$phase.bend([$slider('amount', 1.5, -5, 5), 2])`,
    },
    {
        marked: `out: $saw('a3').track({ gain: [1.|5, 2] })`,
        name: 'a poly element of an API option',
        title: `Wrap 'gain' in $slider`,
        want: `{ gain: [$slider('gain', 1.5, 0, 5), 2] }`,
    },
    {
        marked: `out: $mix([$saw('a3'), 1.|5])`,
        name: 'an item of a list of signals',
        title: `Wrap 'inputs' in $slider`,
        want: `$mix([$saw('a3'), $slider('inputs', 1.5, 0, 5)])`,
    },
    {
        marked: `out: $sine($step([1.|5, 2], $pulse('2hz')))`,
        name: 'a step',
        title: `Wrap 'steps' in $slider`,
        want: `$step([$slider('steps', 1.5, 0, 5), 2]`,
    },
    {
        marked: `out: $sine($keyframes([[1.|5, 0], [2, 1]]))`,
        name: 'a keyframe value',
        title: `Wrap 'keyframes' in $slider`,
        want: `[[$slider('keyframes', 1.5, 0, 5), 0], [2, 1]]`,
    },
    {
        marked: `out: $eq($saw('a3'), { sections: [{ freq: '1000hz', gain: |3 }] })`,
        name: 'a nested option member',
        title: `Wrap 'gain' in $slider`,
        want: `gain: $slider('gain', 3, -24, 24)`,
    },
    {
        marked: `out: $sine($quantizer($saw('1hz'), 'c|4'))`,
        name: 'a signal with a non-signal alternative',
        title: `Wrap 'scale' in a note $slider`,
        want: `$quantizer($saw('1hz'), $slider('scale', 'c4'`,
    },
    {
        marked: `out: $mix([$saw('a3'), $c(1.|5)])`,
        name: 'a $c in a list',
        title: `Replace $c(1.5) with $slider`,
        want: `$mix([$saw('a3'), $slider('inputs', 1.5, 0, 5)])`,
    },
    {
        marked: `out: $pWarp($saw('1hz').range(0, 1), $phase.bend(|))`,
        name: 'a missing $phase amount',
        title: `Add slider for 'amount'`,
        want: `$phase.bend($slider('amount', 0, -5, 5))`,
    },
];

test.describe('control quick-fixes inside arguments', () => {
    test.beforeEach(async ({ window }) => {
        await openUntitledBuffer(window);
    });

    for (const { name, marked, title, want } of RUNNABLE) {
        test(`${name} gets a control and still runs`, async ({ window }) => {
            expect(await applyFix(window, marked, title)).toContain(
                flatten(want),
            );
            expect(await runPatch(window)).toEqual([]);
        });
    }

    test('only the signal positions of a tuple wrap', async ({ window }) => {
        const slice = await applyFix(
            window,
            `out: $sampler('/wavs/x.wav', $pulse('2hz'), { slice: [[0, 0.5], 1.|5] })`,
            `Wrap 'slice' in $slider`,
        );
        expect(slice).toContain(
            flatten(`slice: [[0, 0.5], $slider('slice', 1.5, 0, 5)]`),
        );

        // The call's unset `speed` always offers an add, so the widget opens
        // and its rows show that the slice points offer no wrap.
        await setCode(
            window,
            `out: $sampler('/wavs/x.wav', $pulse('2hz'), { slice: [[0, 0.|5], 1.5] })`,
        );
        await triggerQuickFix(window);
        const rows = window.locator(ROWS);
        await expect(rows.first()).toBeVisible({ timeout: 20_000 });
        const titles = await rows.allInnerTexts();
        expect(titles.some((t) => t.includes(`Add slider for 'speed'`))).toBe(
            true,
        );
        expect(titles.filter((t) => t.includes('$slider'))).toEqual([]);
    });
});
