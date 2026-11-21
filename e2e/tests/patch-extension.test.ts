/**
 * E2E tests for patch file extensions: new patches are saved as `.sl`, and an
 * `.sl` patch reads in the editor exactly as the same patch in `.mjs` does.
 */

import * as fs from 'fs';
import * as path from 'path';
import type { ElectronApplication, Page } from '@playwright/test';
import { test, expect, openUntitledBuffer } from '../fixtures';

/** A patch with a track label and a type error, so the editor has diagnostics to report. */
const PATCH = "bass: $saw('c2')\nconst n: number = 1;\n$sine(undefinedName);\n";

async function workspaceRoot(window: Page): Promise<string> {
    return window.evaluate(
        async () => (await window.electronAPI.filesystem.getWorkspace())!.path,
    );
}

/** Lists `names` as patches in the explorer. */
async function listFiles(electronApp: ElectronApplication, names: string[]) {
    await electronApp.evaluate(({ ipcMain }, files) => {
        ipcMain.removeHandler('modular:fs:list-files');
        ipcMain.handle('modular:fs:list-files', () =>
            files.map((name) => ({
                fileType: 'js',
                name,
                path: name,
                type: 'file',
            })),
        );
    }, names);
}

/** The diagnostics on the open patch, once the TypeScript worker settles. */
async function patchDiagnostics(window: Page): Promise<string> {
    let last = '';
    await expect
        .poll(
            async () => {
                last = await window.evaluate(() => {
                    const model = window.monaco.editor
                        .getEditors()
                        .find(
                            (e) =>
                                e.getModel()?.getLanguageId() === 'javascript',
                        )!
                        .getModel()!;
                    return JSON.stringify(
                        window.monaco.editor
                            .getModelMarkers({ resource: model.uri })
                            .map((m) => [
                                m.startLineNumber,
                                m.startColumn,
                                m.message,
                            ])
                            .sort(),
                    );
                });
                return last;
            },
            { timeout: 15_000 },
        )
        .not.toBe('[]');
    return last;
}

test.describe('patch file extensions', () => {
    test('a new patch is saved as .sl', async ({ electronApp, window }) => {
        const hasTestAPI = await window.evaluate(() => !!window.__TEST_API__);
        test.skip(!hasTestAPI, '__TEST_API__ not available');

        // The save dialog accepts the name it suggests.
        await electronApp.evaluate(({ ipcMain }) => {
            ipcMain.removeHandler('modular:fs:show-save-dialog');
            ipcMain.handle(
                'modular:fs:show-save-dialog',
                (_event, defaultPath: string) => defaultPath,
            );
        });
        const root = await workspaceRoot(window);

        await openUntitledBuffer(window);
        await window.locator('.monaco-editor').first().click();
        await window.keyboard.press('ControlOrMeta+s');

        const saved = path.join(root, 'untitled.sl');
        await expect.poll(() => fs.existsSync(saved)).toBe(true);
        expect(fs.readFileSync(saved, 'utf-8')).toMatch(/@shoreline/);
    });

    test('an .sl patch reads as the same .mjs patch does', async ({
        electronApp,
        window,
    }) => {
        const hasTestAPI = await window.evaluate(() => !!window.__TEST_API__);
        test.skip(!hasTestAPI, '__TEST_API__ not available');

        const root = await workspaceRoot(window);
        fs.writeFileSync(path.join(root, 'song.sl'), PATCH);
        fs.writeFileSync(path.join(root, 'old.mjs'), PATCH);
        await listFiles(electronApp, ['old.mjs', 'song.sl']);
        await window.reload();
        await window.waitForLoadState('networkidle');

        await window.getByText('old.mjs').click();
        await window.waitForSelector('.monaco-editor');
        const mjs = await patchDiagnostics(window);

        await window.getByText('song.sl').click();
        await expect
            .poll(() =>
                window.evaluate(() => window.__TEST_API__!.getEditorValue()),
            )
            .toBe(PATCH);
        const sl = await patchDiagnostics(window);

        expect(sl).toBe(mjs);
    });
});
