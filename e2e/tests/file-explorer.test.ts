/**
 * E2E tests for the explorer's workspace tree: folders expand on click, a
 * click opens a patch, dragging an entry onto a folder moves it on disk with
 * its open buffer, and an asset dragged into the editor inserts its path.
 */

import * as fs from 'fs';
import * as path from 'path';
import type { ElectronApplication, Locator, Page } from '@playwright/test';
import { test, expect } from '../fixtures';

/** Lists the workspace from disk, as the app's own listing does. */
async function listFromDisk(electronApp: ElectronApplication) {
    await electronApp.evaluate(({ ipcMain }) => {
        const nodeFs = process.getBuiltinModule('fs');
        const nodePath = process.getBuiltinModule('path');
        const root = process.env.E2E_WORKSPACE!;
        type Entry = {
            name: string;
            path: string;
            type: 'file' | 'directory';
            fileType?: string;
            children?: Entry[];
        };
        const read = (dir: string, rel: string): Entry[] =>
            nodeFs
                .readdirSync(dir, { withFileTypes: true })
                .flatMap((item): Entry[] => {
                    const itemRel = rel ? `${rel}/${item.name}` : item.name;
                    if (item.isDirectory()) {
                        const children = read(
                            nodePath.join(dir, item.name),
                            itemRel,
                        );
                        return children.length > 0
                            ? [
                                  {
                                      children,
                                      name: item.name,
                                      path: itemRel,
                                      type: 'directory',
                                  },
                              ]
                            : [];
                    }
                    const fileType = item.name.endsWith('.js')
                        ? 'patch'
                        : item.name.endsWith('.wav')
                          ? 'wav'
                          : undefined;
                    return fileType
                        ? [
                              {
                                  fileType,
                                  name: item.name,
                                  path: itemRel,
                                  type: 'file',
                              },
                          ]
                        : [];
                })
                .sort((a, b) =>
                    a.type !== b.type
                        ? a.type === 'directory'
                            ? -1
                            : 1
                        : a.name.localeCompare(b.name),
                );
        ipcMain.removeHandler('modular:fs:list-files');
        ipcMain.handle('modular:fs:list-files', () => read(root, ''));
    });
}

const row = (window: Page, name: string) =>
    window.locator('.workspace-tree .tree-entry', { hasText: name });

const treeNames = (window: Page) =>
    window.locator('.workspace-tree .tree-name').allTextContents();

/**
 * Drags `source` onto the middle of `target` in small steps, as a hand would.
 * A browser repeats dragover while the pointer rests, and the tree accepts a
 * spot only on a later dragover than the one that reached it; an emulated
 * drag sends none of those repeats, so the pointer nudges until `target`
 * shows it will receive the drop, then releases.
 */
async function dragOnto(window: Page, source: Locator, target: Locator) {
    const from = (await source.boundingBox())!;
    const to = (await target.boundingBox())!;
    const y = to.y + to.height / 2;
    await window.mouse.move(from.x + 20, from.y + from.height / 2);
    await window.mouse.down();
    await window.mouse.move(from.x + 25, from.y + from.height / 2, {
        steps: 3,
    });
    await window.mouse.move(to.x + 40, y, { steps: 10 });
    let nudge = 0;
    await expect
        .poll(async () => {
            await window.mouse.move(to.x + 40 + (nudge++ % 2), y);
            return target.getAttribute('class');
        })
        .toMatch(/\bdrop-target\b/);
    await window.mouse.move(to.x + 42, y);
    await window.mouse.up();
}

async function workspaceDir(electronApp: ElectronApplication) {
    return electronApp.evaluate(() => process.env.E2E_WORKSPACE!);
}

test.describe('workspace tree', () => {
    test.beforeEach(async ({ electronApp, window }) => {
        const root = await workspaceDir(electronApp);
        fs.mkdirSync(path.join(root, 'songs'), { recursive: true });
        fs.mkdirSync(path.join(root, 'samples'), { recursive: true });
        fs.writeFileSync(
            path.join(root, 'songs', 'intro.js'),
            '$sine("c4").out()\n',
        );
        fs.writeFileSync(path.join(root, 'loose.js'), '// loose\n');
        fs.writeFileSync(path.join(root, 'samples', 'kick.wav'), '');
        await listFromDisk(electronApp);
        await window.getByTitle('Refresh file tree').click();
        await expect(row(window, 'loose.js')).toBeVisible();
    });

    test('a folder expands on click and a patch opens on click', async ({
        window,
    }) => {
        expect(await treeNames(window)).toEqual([
            'samples',
            'songs',
            'loose.js',
        ]);
        await row(window, 'songs').click();
        expect(await treeNames(window)).toEqual([
            'samples',
            'songs',
            'intro.js',
            'loose.js',
        ]);

        await row(window, 'intro.js').click();
        await expect(window.locator('.monaco-editor').first()).toBeVisible();
        await expect(row(window, 'intro.js')).toHaveClass(/\bactive\b/);
    });

    test('dragging an open file onto a folder moves it with its buffer', async ({
        electronApp,
        window,
    }) => {
        const root = await workspaceDir(electronApp);
        await row(window, 'loose.js').dblclick();
        await expect(window.locator('.monaco-editor').first()).toBeVisible();

        await dragOnto(window, row(window, 'loose.js'), row(window, 'songs'));

        await expect
            .poll(() => fs.existsSync(path.join(root, 'songs', 'loose.js')))
            .toBe(true);
        expect(fs.existsSync(path.join(root, 'loose.js'))).toBe(false);
        await expect(row(window, 'loose.js')).toHaveClass(/\bactive\b/);
        await expect(
            window.locator('.file-list li', { hasText: 'loose.js' }),
        ).toHaveCount(1);
    });

    test('a move never overwrites a file of the same name', async ({
        electronApp,
        window,
    }) => {
        const root = await workspaceDir(electronApp);
        fs.writeFileSync(path.join(root, 'songs', 'loose.js'), '// kept\n');
        await dragOnto(window, row(window, 'loose.js'), row(window, 'songs'));

        await expect(window.getByText('already exists')).toBeVisible();
        expect(
            fs.readFileSync(path.join(root, 'songs', 'loose.js'), 'utf8'),
        ).toBe('// kept\n');
        expect(fs.existsSync(path.join(root, 'loose.js'))).toBe(true);
    });

    test('an asset dragged into the editor inserts its path', async ({
        window,
    }) => {
        await row(window, 'loose.js').dblclick();
        await window.evaluate(() => window.__TEST_API__!.setEditorValue('\n'));
        await row(window, 'samples').click();

        await row(window, 'kick.wav').dragTo(
            window.locator('.monaco-editor .view-lines').first(),
        );

        await expect
            .poll(() =>
                window.evaluate(() =>
                    window.monaco.editor
                        .getEditors()
                        .find(
                            (e) =>
                                e.getModel()?.getLanguageId() === 'javascript',
                        )!
                        .getValue(),
                ),
            )
            .toContain('/samples/kick.wav');
    });
});
