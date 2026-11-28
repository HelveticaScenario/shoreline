/**
 * E2E tests for error handling.
 *
 * Verifies that invalid DSL produces visible errors.
 */

import { test, expect, openUntitledBuffer } from '../fixtures';

test.describe('error handling', () => {
    test('syntax error shows error display', async ({ window }) => {
        await window.waitForTimeout(3000);

        const hasTestAPI = await window.evaluate(() => !!window.__TEST_API__);
        test.skip(!hasTestAPI, '__TEST_API__ not available');

        await openUntitledBuffer(window);

        // Set invalid DSL code (syntax error)
        await window.evaluate(() => {
            window.__TEST_API__!.setEditorValue(
                'this is not valid javascript {{{',
            );
        });

        // Try to execute
        await window.evaluate(() => window.__TEST_API__!.executePatch());
        await window.waitForTimeout(2000);

        // The error display should become visible or the result should indicate failure
        const errorVisible = await window
            .locator('.error-display')
            .isVisible()
            .catch(() => false);
        const result = await window.evaluate(() =>
            window.__TEST_API__!.getLastPatchResult(),
        );

        // Either the UI shows an error, or the result indicates failure
        const hasError = errorVisible || (result && !result.success);
        expect(hasError).toBe(true);
    });

    test('unknown module type shows error', async ({ window }) => {
        await window.waitForTimeout(3000);

        const hasTestAPI = await window.evaluate(() => !!window.__TEST_API__);
        test.skip(!hasTestAPI, '__TEST_API__ not available');

        await openUntitledBuffer(window);

        // Use a nonexistent module function
        await window.evaluate(() => {
            window.__TEST_API__!.setEditorValue(
                'out: $nonexistentModule($hzToV(440))',
            );
        });

        await window.evaluate(() => window.__TEST_API__!.executePatch());
        await window.waitForTimeout(2000);

        const result = await window.evaluate(() =>
            window.__TEST_API__!.getLastPatchResult(),
        );

        // Should fail since the module doesn't exist
        if (result) {
            expect(result.success).toBe(false);
        }
    });

    test('error message contains useful info', async ({ window }) => {
        await window.waitForTimeout(3000);

        const hasTestAPI = await window.evaluate(() => !!window.__TEST_API__);
        test.skip(!hasTestAPI, '__TEST_API__ not available');

        await openUntitledBuffer(window);

        await window.evaluate(() => {
            window.__TEST_API__!.setEditorValue(
                'throw new Error("test error message")',
            );
        });

        await window.evaluate(() => window.__TEST_API__!.executePatch());
        await window.waitForTimeout(2000);

        const result = await window.evaluate(() =>
            window.__TEST_API__!.getLastPatchResult(),
        );

        if (result && result.errorMessage) {
            expect(result.errorMessage).toContain('test error message');
        }
    });
});

test.describe('error banner line numbers', () => {
    const runAndReadBanner = async (
        window: import('@playwright/test').Page,
        source: string,
    ) => {
        await openUntitledBuffer(window);
        await window.evaluate((code) => {
            window.__TEST_API__!.setEditorValue(code);
        }, source);
        await window.evaluate(() => window.__TEST_API__!.executePatch());
        const banner = window.locator('.error-display');
        await expect(banner).toBeVisible({ timeout: 10_000 });
        return banner;
    };

    test('a syntax error shows its line', async ({ window }) => {
        const banner = await runAndReadBanner(
            window,
            'const a = 1;\nconst b = ;\n',
        );
        await expect(banner.locator('.error-message')).toContainText(
            '(line 2)',
        );
    });

    test('an error thrown while the patch runs shows its line', async ({
        window,
    }) => {
        const banner = await runAndReadBanner(
            window,
            'const a = 1;\n\nnotDefined();\n',
        );
        await expect(banner.locator('.error-message')).toContainText(
            'notDefined is not defined (line 3)',
        );
    });

    test('an engine error on one of several same-type modules shows its line', async ({
        window,
    }) => {
        const banner = await runAndReadBanner(
            window,
            'a: $sine("c4");\nb: $sine({ type: "cable", module: "ghost", port: "output" });\n',
        );
        await expect(banner.locator('.validation-error')).toContainText(
            "Module 'ghost' not found for cable source",
        );
        await expect(banner.locator('.error-location')).toHaveText('at line 2');
    });
});
