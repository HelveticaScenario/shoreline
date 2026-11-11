/**
 * E2E tests for output-resolution inlay hints: the editor names the output a
 * multi-output module stands for at each place it is used.
 */

import type { Page } from '@playwright/test';
import { test, expect, openUntitledBuffer } from '../fixtures';

/**
 * The editor's rendered lines, inlay hints included, with Monaco's
 * non-breaking spaces normalised.
 */
async function renderedText(window: Page): Promise<string> {
    return window.evaluate(() =>
        [...document.querySelectorAll('.monaco-editor .view-line')]
            .map((line) => ({
                text: (line.textContent ?? '').replace(/ /g, ' '),
                top: (line as HTMLElement).offsetTop,
            }))
            .sort((a, b) => a.top - b.top)
            .map((line) => line.text)
            .join('\n'),
    );
}

async function setSource(window: Page, source: string): Promise<void> {
    await window.evaluate(
        (value) => window.__TEST_API__!.setEditorValue(value),
        source,
    );
}

test.describe('output resolution hints', () => {
    test.beforeEach(async ({ window }) => {
        await window.waitForTimeout(3000);
        const hasTestAPI = await window.evaluate(() => !!window.__TEST_API__);
        test.skip(!hasTestAPI, '__TEST_API__ not available');
        await openUntitledBuffer(window);
    });

    test('a sequencer shows the output each input takes, before the patch runs', async ({
        window,
    }) => {
        await setSource(
            window,
            `const seq = $p('c e g').c\n$saw(seq).amp($perc(seq)).out()`,
        );
        await expect
            .poll(() => renderedText(window), { timeout: 20_000 })
            .toContain('$saw(seq.cv).amp($perc(seq.trig)).out()');
    });

    test('a chain shows the hint after its receiver', async ({ window }) => {
        await setSource(window, `$p('0*8').c.$.perc().out()`);
        await expect
            .poll(() => renderedText(window), { timeout: 20_000 })
            .toContain(`$p('0*8').c.trig.$.perc().out()`);
    });

    test('an array built in a loop shows hints before the patch runs', async ({
        window,
    }) => {
        await setSource(
            window,
            `const seqs = []\nfor (let i = 0; i < 5; i++) {\n    seqs.push($p('c e g').c)\n}\n$sine(seqs).amp($perc(seqs)).out()`,
        );
        await expect
            .poll(() => renderedText(window), { timeout: 20_000 })
            .toContain('$sine(seqs.cv).amp($perc(seqs.trig)).out()');
    });

    test('an untyped helper parameter is hinted once the patch has run', async ({
        window,
    }) => {
        const source = `function voice(s) {\n    return $saw(s).amp($perc(s))\n}\nvoice($p('c e g').c).out()`;
        await setSource(window, source);
        // Nothing can be known about `s` until the patch runs.
        await window.waitForTimeout(3000);
        expect(await renderedText(window)).toContain(
            'return $saw(s).amp($perc(s))',
        );

        await window.evaluate(() => window.__TEST_API__!.executePatch());
        await expect
            .poll(() => renderedText(window), { timeout: 20_000 })
            .toContain('return $saw(s.cv).amp($perc(s.trig))');

        // The hints follow the code when a line is added above it.
        await window.evaluate(() => {
            const monaco = (
                window as unknown as {
                    monaco: typeof import('monaco-editor');
                }
            ).monaco;
            const patchEditor = monaco.editor
                .getEditors()
                .find((e) => e.getModel()?.getLanguageId() === 'javascript')!;
            patchEditor.executeEdits('test', [
                { range: new monaco.Range(1, 1, 1, 1), text: '// a voice\n' },
            ]);
        });
        await window.waitForTimeout(3000);
        expect(await renderedText(window)).toContain(
            'return $saw(s.cv).amp($perc(s.trig))',
        );
    });
});
