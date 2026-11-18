/**
 * E2E tests for Rename Symbol on track labels: the rename is of the track's
 * name alone, wherever the label's prefix leaves the cursor, and reaches the
 * labels that name the track as their group.
 */

import type { Page } from '@playwright/test';
import { test, expect, openUntitledBuffer } from '../fixtures';

async function setSource(window: Page, text: string): Promise<void> {
    await window.evaluate(
        (value) => window.__TEST_API__!.setEditorValue(value),
        text,
    );
}

function source(window: Page): Promise<string> {
    return window.evaluate(() => {
        return (
            window.monaco.editor
                .getEditors()
                .find((e) => e.getModel()?.getLanguageId() === 'javascript')
                ?.getModel()
                ?.getValue() ?? ''
        );
    });
}

/** Asks for a rename at `offset`. */
async function requestRename(window: Page, offset: number): Promise<void> {
    await window.evaluate((at) => {
        const editor = window.monaco.editor
            .getEditors()
            .find((e) => e.getModel()?.getLanguageId() === 'javascript')!;
        editor.setPosition(editor.getModel()!.getPositionAt(at));
        editor.focus();
        editor.trigger('test', 'editor.action.rename', null);
    }, offset);
}

/** Starts a rename at `offset` and returns the rename box's input. */
async function startRename(window: Page, offset: number) {
    await requestRename(window, offset);
    const input = window.locator('.rename-box .rename-input');
    await expect(input).toBeVisible({ timeout: 20_000 });
    return input;
}

test.describe('renaming a track', () => {
    test.beforeEach(async ({ window }) => {
        await window.waitForTimeout(3000);
        const hasTestAPI = await window.evaluate(() => !!window.__TEST_API__);
        test.skip(!hasTestAPI, '__TEST_API__ not available');
        await openUntitledBuffer(window);
    });

    test('a rename on a label offers the name without its prefix', async ({
        window,
    }) => {
        await setSource(
            window,
            `_kick$2: $sine('c1')\nbass: $saw('c2').amp(kick)`,
        );
        const input = await startRename(window, 0);
        await expect(input).toHaveValue('kick');
        await input.fill('thump');
        await input.press('Enter');
        await expect
            .poll(() => source(window))
            .toBe(`_thump$2: $sine('c1')\nbass: $saw('c2').amp(thump)`);
    });

    test('a rename from a use renames the label', async ({ window }) => {
        const patch = `S__kick: $sine('c1')\nbass: $saw('c2').amp(kick)`;
        await setSource(window, patch);
        const input = await startRename(window, patch.lastIndexOf('kick') + 1);
        await expect(input).toHaveValue('kick');
        await input.fill('thump');
        await input.press('Enter');
        await expect
            .poll(() => source(window))
            .toBe(`S__thump: $sine('c1')\nbass: $saw('c2').amp(thump)`);
    });

    test('renaming a group renames the labels that play into it', async ({
        window,
    }) => {
        const patch = `drums: $group()\n_kick$drums: $sine('c1')\nhat$drums: $noise().amp(drums)`;
        await setSource(window, patch);
        const input = await startRename(window, 2);
        await expect(input).toHaveValue('drums');
        await input.fill('kit');
        await input.press('Enter');
        await expect
            .poll(() => source(window))
            .toBe(
                `kit: $group()\n_kick$kit: $sine('c1')\nhat$kit: $noise().amp(kit)`,
            );
    });

    test('a rename on a group suffix renames the group', async ({ window }) => {
        const patch = `drums: $group()\nkick$drums: $sine('c1')\nhat$drums: $noise()`;
        await setSource(window, patch);
        const input = await startRename(window, patch.indexOf('$drums') + 3);
        await expect(input).toHaveValue('drums');
        await input.fill('kit');
        await input.press('Enter');
        await expect
            .poll(() => source(window))
            .toBe(`kit: $group()\nkick$kit: $sine('c1')\nhat$kit: $noise()`);
    });

    test('a suffix naming no track cannot be renamed', async ({ window }) => {
        const patch = `kick$drums: $sine('c1')`;
        await setSource(window, patch);
        await requestRename(window, patch.indexOf('drums') + 2);
        await expect(
            window.locator('.monaco-editor-overlaymessage'),
        ).toContainText('You cannot rename this element.');
    });

    test('a name with a prefix is refused', async ({ window }) => {
        const patch = `kick: $sine('c1')\nbass: $saw('c2').amp(kick)`;
        await setSource(window, patch);
        const input = await startRename(window, 1);
        await input.fill('_thump');
        await input.press('Enter');
        await expect(
            window.locator('.monaco-editor-overlaymessage'),
        ).toContainText('is not a track name');
        expect(await source(window)).toBe(patch);
    });
});
