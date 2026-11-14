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
            `const seq = $('c e g')\n$saw(seq).amp($perc(seq)).out()`,
        );
        await expect
            .poll(() => renderedText(window), { timeout: 20_000 })
            .toContain('$saw(seq._cv).amp($perc(seq._trig)).out()');
    });

    test('a chain shows the hint after its receiver', async ({ window }) => {
        await setSource(window, `$('0*8').perc().out()`);
        await expect
            .poll(() => renderedText(window), { timeout: 20_000 })
            .toContain(`$('0*8')._trig.perc().out()`);
    });

    test('an array built in a loop shows hints before the patch runs', async ({
        window,
    }) => {
        await setSource(
            window,
            `const seqs = []\nfor (let i = 0; i < 5; i++) {\n    seqs.push($('c e g'))\n}\n$sine(seqs).amp($perc(seqs)).out()`,
        );
        await expect
            .poll(() => renderedText(window), { timeout: 20_000 })
            .toContain('$sine(seqs._cv).amp($perc(seqs._trig)).out()');
    });

    test('an untyped helper parameter is hinted once the patch has run', async ({
        window,
    }) => {
        const source = `function voice(s) {\n    return $saw(s).amp($perc(s))\n}\nvoice($('c e g')).out()`;
        await setSource(window, source);
        // Nothing can be known about `s` until the patch runs.
        await window.waitForTimeout(3000);
        expect(await renderedText(window)).toContain(
            'return $saw(s).amp($perc(s))',
        );

        await window.evaluate(() => window.__TEST_API__!.executePatch());
        await expect
            .poll(() => renderedText(window), { timeout: 20_000 })
            .toContain('return $saw(s._cv).amp($perc(s._trig))');

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
            'return $saw(s._cv).amp($perc(s._trig))',
        );
    });

    test('every kind of pattern is hinted as the sequencer it plays through', async ({
        window,
    }) => {
        const patterns = [
            `$('0 2').scale('c(maj)')`,
            `$('0 2').scale('c(maj)').trans('1')`,
            `$arrange([2, $('c')], [1, $('e')])`,
            `$('c e').fast(2)`,
            `$('c e').slow(2)`,
            `$('c e').struct('x ~ x x')`,
            `$('c').beat('0,7', 16)`,
            `$('c e').glide(0.1)`,
        ];
        await setSource(
            window,
            patterns.map((pattern) => `$perc(${pattern}).out()`).join('\n'),
        );
        for (const pattern of patterns) {
            await expect
                .poll(() => renderedText(window), { timeout: 20_000 })
                .toContain(`$perc(${pattern}._trig).out()`);
        }
    });

    test('a module, and a pattern passed to $cycle, are hinted alike', async ({
        window,
    }) => {
        await setSource(
            window,
            `const seq = $cycle($('c e g'))\n$adsr(seq).out()\n$perc($clock).out()`,
        );
        await expect
            .poll(() => renderedText(window), { timeout: 20_000 })
            .toContain(`$adsr(seq._gate).out()`);
        const text = await renderedText(window);
        // The pattern is `$cycle`'s pattern here, not a signal.
        expect(text).toContain(`const seq = $cycle($('c e g'))`);
        expect(text).toMatch(/\$perc\(\$clock\._\w+\)\.out\(\)/);
    });

    test('a named output shows no hint', async ({ window }) => {
        await setSource(
            window,
            `const seq = $('c e g')\n$saw(seq).amp($perc(seq._gate)).out()`,
        );
        await expect
            .poll(() => renderedText(window), { timeout: 20_000 })
            .toContain('$saw(seq._cv).amp($perc(seq._gate)).out()');
    });

    test('double-clicking a hint writes the output into the code', async ({
        window,
    }) => {
        await setSource(window, `const seq = $('c e g')\n$perc(seq).out()`);
        await expect
            .poll(() => renderedText(window), { timeout: 20_000 })
            .toContain('$perc(seq._trig).out()');
        await window
            .locator('.monaco-editor .view-line span', { hasText: /^\._trig$/ })
            .first()
            .dblclick();
        await expect
            .poll(
                () =>
                    window.evaluate(() =>
                        window.__TEST_API__!.getEditorValue(),
                    ),
                { timeout: 10_000 },
            )
            .toContain('$perc(seq._trig).out()');
        // The written output runs as that output.
        await window.evaluate(() => window.__TEST_API__!.executePatch());
        await expect
            .poll(
                () =>
                    window.evaluate(
                        () =>
                            window.__TEST_API__!.getLastPatchResult() !== null,
                    ),
                { timeout: 20_000 },
            )
            .toBe(true);
    });
});
