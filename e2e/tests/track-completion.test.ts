/**
 * E2E test for completion on a patch with track labels: it offers the track
 * names and none of the names the editor declares for itself.
 */

import { test, expect, openUntitledBuffer } from '../fixtures';

test.describe('completion with track labels', () => {
    test.beforeEach(async ({ window }) => {
        await window.waitForTimeout(3000);
        const hasTestAPI = await window.evaluate(() => !!window.__TEST_API__);
        test.skip(!hasTestAPI, '__TEST_API__ not available');
        await openUntitledBuffer(window);
    });

    test("the editor's own names are not offered", async ({ window }) => {
        const patch = `kick_drum: $sine('c1')\nconst __mine = 1\n`;
        await window.evaluate(
            (value) => window.__TEST_API__!.setEditorValue(value),
            patch,
        );
        const labels = async (typed: string) => {
            await window.evaluate(
                ({ code, text }) => {
                    const editor = window.monaco.editor
                        .getEditors()
                        .find(
                            (e) =>
                                e.getModel()?.getLanguageId() === 'javascript',
                        )!;
                    editor.setValue(code + text);
                    editor.setPosition(
                        editor
                            .getModel()!
                            .getPositionAt(code.length + text.length),
                    );
                    editor.focus();
                    editor.trigger(
                        'test',
                        'editor.action.triggerSuggest',
                        null,
                    );
                },
                { code: patch, text: typed },
            );
            const rows = window.locator(
                '.suggest-widget .monaco-list-row .label-name',
            );
            await expect(rows.first()).toBeVisible({ timeout: 20_000 });
            return rows.allInnerTexts();
        };
        expect(await labels('kick')).toContain('kick_drum');
        const underscored = await labels('__');
        expect(underscored).toContain('__mine');
        expect(underscored.filter((l) => l.startsWith('__'))).toEqual([
            '__mine',
        ]);
    });
});
