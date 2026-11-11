/**
 * E2E tests for the module reference page of the docs.
 */

import { test, expect } from '../fixtures';

test.describe('module reference', () => {
    test('the module index width survives a reload', async ({ window }) => {
        await window.getByRole('button', { name: 'Docs' }).click();
        const index = window.locator('#docs-module-index');
        await expect(index).toBeVisible({ timeout: 15_000 });
        const measure = async () =>
            Math.round((await index.boundingBox())!.width);
        const before = await measure();
        await window
            .getByRole('separator', { name: 'Resize module index' })
            .focus();
        await window.keyboard.press('ArrowRight');
        await window.keyboard.press('ArrowRight');
        await expect.poll(measure).toBe(before + 32);

        await window.reload();
        await window.waitForLoadState('domcontentloaded');

        await expect(index).toBeVisible({ timeout: 15_000 });
        await expect.poll(measure).toBe(before + 32);
    });
});
