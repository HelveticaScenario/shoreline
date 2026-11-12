/**
 * E2E tests for the settings panel in the sidebar.
 */

import { test, expect } from '../fixtures';

const OPEN_SETTINGS = process.platform === 'darwin' ? 'Meta+,' : 'Control+,';

test.describe('settings panel', () => {
    test('Cmd+comma shows settings in the sidebar', async ({ window }) => {
        await window.keyboard.press(OPEN_SETTINGS);

        await expect(window.locator('.settings-sidebar')).toBeVisible();
        await expect(window.locator('#app-sidebar-title')).toHaveText(
            'Settings',
        );
    });

    test('the activity bar opens settings', async ({ window }) => {
        await window.getByRole('button', { name: 'Settings' }).click();

        await expect(window.locator('.settings-sidebar')).toBeVisible();
    });

    test('the audio tab shows the output device', async ({ window }) => {
        await window.keyboard.press(OPEN_SETTINGS);
        await window.getByRole('tab', { name: 'Audio' }).click();

        await expect(
            window.getByRole('heading', { name: 'Output Device' }),
        ).toBeVisible();
    });

    test('another panel hides settings', async ({ window }) => {
        await window.keyboard.press(OPEN_SETTINGS);
        await window.getByRole('button', { name: 'Explorer' }).click();

        await expect(window.locator('.settings-sidebar')).toBeHidden();
    });

    test('the sidebar side setting moves the sidebar', async ({ window }) => {
        await window.keyboard.press(OPEN_SETTINGS);
        const sidebar = window.locator('.app-sidebar');
        const sideSelect = window
            .locator('.settings-section', { hasText: 'Sidebar Side' })
            .locator('select');
        const windowWidth = await window.evaluate(() => window.innerWidth);

        await sideSelect.selectOption('left');
        await expect(sidebar).toHaveClass(/\bleft\b/);
        expect((await sidebar.boundingBox())?.x).toBe(0);

        await sideSelect.selectOption('right');
        await expect(sidebar).toHaveClass(/\bright\b/);
        const box = await sidebar.boundingBox();
        expect(Math.round((box?.x ?? 0) + (box?.width ?? 0))).toBe(
            windowWidth,
        );
    });
});
