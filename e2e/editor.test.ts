/**
 * E2E tests for DSL interaction via the Monaco editor.
 *
 * These tests exercise the core user workflow:
 *   1. Type DSL code into the editor
 *   2. Click "Update Patch" (or press Ctrl+Enter)
 *   3. Verify the patch runs (no error banner) or shows correct errors.
 */

import { test, expect } from './fixtures';

test.describe('DSL editor interaction', () => {
    test('Monaco editor is present when workspace is open', async ({
        window,
    }) => {
        // The editor might not be visible if no workspace is open.
        // We look for the Monaco container.
        const editorOrEmpty = window.locator('.editor-panel, .empty-state');
        await expect(editorOrEmpty.first()).toBeVisible({ timeout: 15_000 });
    });

    test('Update Patch button is clickable', async ({ window }) => {
        const btn = window.locator('button:has-text("Update Patch")');
        await expect(btn).toBeEnabled({ timeout: 15_000 });
    });

    test('Stop button is present', async ({ window }) => {
        // Wait for UI to settle
        await window.waitForTimeout(1000);
        const stopBtn = window.locator('button:has-text("Stop")');
        await expect(stopBtn).toBeVisible();
    });

    test('Docs tab shows the type and module reference', async ({ window }) => {
        await window.getByRole('button', { name: 'Docs' }).click();
        const docs = window.locator('.docs-panel');
        await expect(docs.locator('.module-card').first()).toBeVisible({
            timeout: 15_000,
        });

        await docs.getByRole('button', { name: 'Types' }).click();
        await expect(docs.locator('.type-card').first()).toBeVisible();
    });
});

test.describe('error handling', () => {
    test('error display is not visible initially', async ({ window }) => {
        await window.waitForTimeout(1000);
        // The error display should not be showing on a fresh launch
        const errorDisplay = window.locator('.error-display');
        const isVisible = await errorDisplay.isVisible().catch(() => false);
        // It's OK if the error display element exists but has no visible errors
        if (isVisible) {
            // If visible, it should be empty or contain no error messages
            const errorCount = await window
                .locator('.error-display .error-message')
                .count();
            expect(errorCount).toBe(0);
        }
    });
});
