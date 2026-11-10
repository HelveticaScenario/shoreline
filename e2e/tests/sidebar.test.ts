/**
 * E2E tests for opening and closing the sidebar from the activity bar.
 */

import { test, expect } from '../fixtures';

test.describe('sidebar activity bar', () => {
    test('clicking the shown panel closes the sidebar', async ({ window }) => {
        const panel = window.locator('#app-sidebar-panel');
        const explorer = window.getByRole('button', { name: 'Explorer' });

        await expect(panel).toBeVisible();
        await explorer.click();
        await expect(panel).toBeHidden();
        await expect(explorer).toHaveAttribute('aria-pressed', 'false');
        await expect(window.getByRole('separator')).toHaveCount(0);

        await explorer.click();
        await expect(panel).toBeVisible();
        await expect(explorer).toHaveAttribute('aria-pressed', 'true');
    });

    test('a closed sidebar stays closed after a reload', async ({ window }) => {
        const panel = window.locator('#app-sidebar-panel');

        await window.getByRole('button', { name: 'Explorer' }).click();
        await expect(panel).toBeHidden();
        await window.reload();
        await window.waitForLoadState('networkidle');
        await expect(
            window.getByRole('button', { name: 'Explorer' }),
        ).toBeVisible();
        await expect(panel).toBeHidden();

        await window.getByRole('button', { name: 'Explorer' }).click();
        await window.reload();
        await window.waitForLoadState('networkidle');
        await expect(panel).toBeVisible();
    });

    test('another panel reopens a closed sidebar', async ({ window }) => {
        const panel = window.locator('#app-sidebar-panel');

        await window.getByRole('button', { name: 'Explorer' }).click();
        await expect(panel).toBeHidden();
        await window.getByRole('button', { name: 'Docs' }).click();
        await expect(panel).toBeVisible();
        await expect(window.locator('#app-sidebar-title')).toHaveText('Docs');
    });

    test('opening settings reopens a closed sidebar', async ({ window }) => {
        await window.getByRole('button', { name: 'Explorer' }).click();
        await window.keyboard.press(
            process.platform === 'darwin' ? 'Meta+,' : 'Control+,',
        );
        await expect(window.locator('.settings-sidebar')).toBeVisible();
    });
});
