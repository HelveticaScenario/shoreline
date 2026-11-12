/**
 * E2E tests for the bottom bar and the panel it opens under the editor.
 */

import { test, expect } from '../fixtures';

const OPEN_SETTINGS = process.platform === 'darwin' ? 'Meta+,' : 'Control+,';

test.describe('bottom panel', () => {
    test('the bar spans the window and the panel the editor column', async ({
        window,
    }) => {
        const bar = window.locator('.app-bottom-bar');
        const panel = window.locator('#app-bottom-panel');
        const editor = window.locator('.editor-panel');
        const windowWidth = await window.evaluate(() => window.innerWidth);
        const sideSelect = window
            .locator('.settings-section', { hasText: 'Sidebar Side' })
            .locator('select');

        await window.getByRole('button', { name: 'Mixer' }).click();
        await expect(panel).toBeVisible();

        await window.keyboard.press(OPEN_SETTINGS);
        for (const side of ['left', 'right']) {
            await sideSelect.selectOption(side);
            await expect(window.locator('.app-sidebar')).toHaveClass(
                new RegExp(`\\b${side}\\b`),
            );
            const barBox = await bar.boundingBox();
            expect(barBox?.x).toBe(0);
            expect(Math.round(barBox?.width ?? 0)).toBe(windowWidth);

            const panelBox = await panel.boundingBox();
            const editorBox = await editor.boundingBox();
            expect(panelBox?.x).toBe(editorBox?.x);
            expect(panelBox?.width).toBe(editorBox?.width);
            // The panel sits between the editor and the bar.
            expect(panelBox!.y).toBeGreaterThanOrEqual(
                editorBox!.y + editorBox!.height,
            );
            expect(panelBox!.y + panelBox!.height).toBeLessThanOrEqual(
                barBox!.y,
            );
        }
    });

    test('one panel shows at a time and its button collapses it', async ({
        window,
    }) => {
        // The docs page has buttons of its own, such as the mixer category.
        const bar = window.getByRole('navigation', { name: 'Bottom panels' });
        const mixer = bar.getByRole('button', { name: 'Mixer' });
        const docs = bar.getByRole('button', { name: 'Docs' });
        const panel = window.locator('#app-bottom-panel');

        await expect(panel).toBeHidden();

        await mixer.click();
        await expect(window.locator('.vu-meter-panel')).toBeVisible();
        await expect(mixer).toHaveAttribute('aria-pressed', 'true');

        await docs.click();
        await expect(window.locator('.docs-panel')).toBeVisible();
        await expect(window.locator('.vu-meter-panel')).toHaveCount(0);
        await expect(mixer).toHaveAttribute('aria-pressed', 'false');

        await docs.click();
        await expect(panel).toBeHidden();
    });

    test('Cmd+Shift+M toggles the mixer', async ({ window }) => {
        const chord = process.platform === 'darwin' ? 'Meta' : 'Control';
        await window.keyboard.press(`${chord}+Shift+M`);
        await expect(window.locator('.vu-meter-panel')).toBeVisible();
        await window.keyboard.press(`${chord}+Shift+M`);
        await expect(window.locator('#app-bottom-panel')).toBeHidden();
    });

    test('the open panel and its height survive a reload', async ({
        window,
    }) => {
        await window.getByRole('button', { name: 'Docs' }).click();
        const sash = window.getByRole('separator', {
            name: 'Resize bottom panel',
        });
        const panel = window.locator('#app-bottom-panel');
        const measure = async () =>
            Math.round((await panel.boundingBox())!.height);
        const before = await measure();
        await sash.focus();
        await window.keyboard.press('ArrowUp');
        await window.keyboard.press('ArrowUp');
        await expect.poll(measure).toBe(before + 32);
        const height = await measure();

        // The height is written once the drag settles.
        await window.waitForTimeout(600);
        await window.reload();
        await window.waitForLoadState('domcontentloaded');

        await expect(window.locator('.docs-panel')).toBeVisible();
        await expect.poll(measure).toBe(height);
    });

    test('the docs location setting moves the docs to the sidebar', async ({
        window,
    }) => {
        const bottomBar = window.locator('.app-bottom-bar');
        const activityBar = window.locator('.app-activity-bar');
        await window.keyboard.press(OPEN_SETTINGS);
        await window
            .locator('.settings-section', { hasText: 'Docs Location' })
            .locator('select')
            .selectOption('sidebar');

        await expect(
            bottomBar.getByRole('button', { name: 'Docs' }),
        ).toHaveCount(0);
        await activityBar.getByRole('button', { name: 'Docs' }).click();
        await expect(
            window.locator('#app-sidebar-panel .docs-panel'),
        ).toBeVisible();
        await expect(window.locator('#app-bottom-panel')).toBeHidden();
    });
});
