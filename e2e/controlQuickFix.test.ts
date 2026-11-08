/**
 * Control quick-fixes in the running editor: the lightbulb offers a slider
 * sized from the param's control range, applying it rewrites the source, and
 * an audio input offers none.
 */

import type { Page } from '@playwright/test';
import { test, expect, openUntitledBuffer } from './fixtures';

/** Put `code` in the patch editor with the caret at the `|` marker. */
async function setCode(page: Page, marked: string) {
    const column = marked.indexOf('|') + 1;
    const code = marked.replace('|', '');
    await page.evaluate(
        ({ code, column }) => {
            const ed = window.monaco.editor
                .getEditors()
                .find((e) => e.getModel()?.getLanguageId() === 'javascript')!;
            ed.setValue(code);
            ed.setPosition({ column, lineNumber: 1 });
            ed.focus();
        },
        { code, column },
    );
}

async function quickFixTitles(page: Page): Promise<string[]> {
    await page.evaluate(() => {
        const ed = window.monaco.editor
            .getEditors()
            .find((e) => e.getModel()?.getLanguageId() === 'javascript')!;
        ed.trigger('e2e', 'editor.action.quickFix', {});
    });
    const rows = page.locator('.action-widget .monaco-list-row');
    await expect(rows.first()).toBeVisible({ timeout: 20_000 });
    return rows.allInnerTexts();
}

async function editorValue(page: Page): Promise<string> {
    return page.evaluate(
        () =>
            window.monaco.editor
                .getEditors()
                .find((e) => e.getModel()?.getLanguageId() === 'javascript')!
                .getValue(),
    );
}

test.describe('control quick-fixes', () => {
    test.beforeEach(async ({ window }) => {
        await openUntitledBuffer(window);
    });

    test('wraps a control literal in a slider over its control range', async ({
        window,
    }) => {
        await setCode(window, `$lpf($noise('white'), 'c4', 2.|5).out()`);
        const titles = await quickFixTitles(window);
        const wrap = titles.findIndex((t) =>
            t.includes(`Wrap 'resonance' in $slider`),
        );
        expect(wrap, titles.join('\n')).toBeGreaterThanOrEqual(0);
        await window
            .locator('.action-widget .monaco-list-row')
            .nth(wrap)
            // The widget's pointer-block overlay swallows real clicks.
            .dispatchEvent('click');
        await expect
            .poll(() => editorValue(window))
            .toContain(`$slider('resonance', 2.5, 0, 5)`);
    });

    test('an audio input offers no wrap', async ({ window }) => {
        await setCode(window, `$lpf(0.|5, 'c4').out()`);
        await window.evaluate(() => {
            const ed = window.monaco.editor
                .getEditors()
                .find((e) => e.getModel()?.getLanguageId() === 'javascript')!;
            ed.trigger('e2e', 'editor.action.quickFix', {});
        });
        await window.waitForTimeout(1500);
        const titles = await window
            .locator('.action-widget .monaco-list-row')
            .allInnerTexts();
        expect(titles.filter((t) => t.includes(`Wrap 'input'`))).toEqual([]);
    });

    test('API method params still get controls', async ({ window }) => {
        await setCode(window, `$saw('a3').out(|)`);
        const titles = await quickFixTitles(window);
        expect(titles.some((t) => t.includes(`'gain'`)), titles.join('\n')).toBe(
            true,
        );
    });
});
