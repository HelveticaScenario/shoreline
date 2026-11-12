/**
 * E2E tests for the Engine Health modal.
 */

import { test, expect } from '../fixtures';

test.describe('engine health modal', () => {
    test('engine health modal opens and displays content', async ({
        window,
    }) => {
        await window.waitForTimeout(2000);

        // Open via test API
        await window.evaluate(() =>
            (window as any).__TEST_API__.openEngineHealth(),
        );
        await window.waitForTimeout(500);

        const panel = window.locator('.engine-health-panel');
        await expect(panel).toBeVisible();
        // Scoped and exact: module names elsewhere in the window contain these
        // words ("$mi.speak").
        for (const label of ['Engine Health', 'Audio CPU', 'Average', 'Peak']) {
            await expect(
                panel.getByText(label, { exact: true }).first(),
            ).toBeVisible();
        }
    });

    test('engine health modal closes on Escape', async ({ window }) => {
        await window.waitForTimeout(2000);

        await window.evaluate(() =>
            (window as any).__TEST_API__.openEngineHealth(),
        );
        await window.waitForTimeout(500);

        const panel = window.locator('.engine-health-panel');
        await expect(panel).toBeVisible();

        await window.keyboard.press('Escape');
        await window.waitForTimeout(500);
        await expect(panel).not.toBeVisible();
    });
});
