/**
 * E2E tests for patch editing workflows.
 *
 * Verifies that patches can be modified and re-executed.
 */

import { test, expect, openUntitledBuffer } from '../fixtures';

test.describe('patch editing', () => {
    test('can execute, modify, and re-execute a patch', async ({ window }) => {
        await window.waitForTimeout(3000);

        const hasTestAPI = await window.evaluate(() => !!window.__TEST_API__);
        test.skip(!hasTestAPI, '__TEST_API__ not available');

        await openUntitledBuffer(window);

        // Execute initial patch
        await window.evaluate(() => {
            window.__TEST_API__!.setEditorValue('out: $sine($hzToV(440))');
        });
        await window.evaluate(() => window.__TEST_API__!.executePatch());
        await window.waitForTimeout(2000);

        const result1 = await window.evaluate(() =>
            window.__TEST_API__!.getLastPatchResult(),
        );
        if (result1) {
            expect(result1.success).toBe(true);
        }

        // Modify and re-execute with different frequency
        await window.evaluate(() => {
            window.__TEST_API__!.setEditorValue('out: $saw($hzToV(220))');
        });
        await window.evaluate(() => window.__TEST_API__!.executePatch());
        await window.waitForTimeout(2000);

        const result2 = await window.evaluate(() =>
            window.__TEST_API__!.getLastPatchResult(),
        );
        if (result2) {
            expect(result2.success).toBe(true);
        }

        // Audio should still be running
        const running = await window.evaluate(() => {
            return window.__TEST_API__!.isEngineRunning();
        });
        expect(running).toBe(true);
    });

    test('re-execution preserves audio state', async ({ window }) => {
        await window.waitForTimeout(3000);

        const hasTestAPI = await window.evaluate(() => !!window.__TEST_API__);
        test.skip(!hasTestAPI, '__TEST_API__ not available');

        await openUntitledBuffer(window);

        // Execute a patch
        await window.evaluate(() => {
            window.__TEST_API__!.setEditorValue('out: $sine($hzToV(440))');
        });
        await window.evaluate(() => window.__TEST_API__!.executePatch());
        await window.waitForTimeout(1000);

        // Verify audio is running
        let running = await window.evaluate(() =>
            window.__TEST_API__!.isEngineRunning(),
        );
        expect(running).toBe(true);

        // Re-execute same patch (should not stop audio)
        await window.evaluate(() => window.__TEST_API__!.executePatch());
        await window.waitForTimeout(1000);

        running = await window.evaluate(() =>
            window.__TEST_API__!.isEngineRunning(),
        );
        expect(running).toBe(true);
    });

    test('stop halts the transport, then silences the engine', async ({
        window,
    }) => {
        await window.waitForTimeout(3000);

        const hasTestAPI = await window.evaluate(() => !!window.__TEST_API__);
        test.skip(!hasTestAPI, '__TEST_API__ not available');

        await openUntitledBuffer(window);

        // Execute a patch first
        await window.evaluate(() => {
            window.__TEST_API__!.setEditorValue('out: $sine($hzToV(440))');
        });
        await window.evaluate(() => window.__TEST_API__!.executePatch());
        await window.waitForTimeout(2000);

        const stopBtn = window.locator('button:has-text("Stop")');
        const engineRunning = () =>
            window.evaluate(() => window.__TEST_API__!.isEngineRunning());

        await stopBtn.click();
        await window.waitForTimeout(500);
        expect(await engineRunning()).toBe(true);

        await stopBtn.click();
        await expect.poll(engineRunning).toBe(false);
    });
});
