/**
 * Docs examples are read-only editors with the patch editor's language
 * features, and they share nothing with the patch buffers.
 */

import { test, expect, openUntitledBuffer } from './fixtures';

const HOVER = '.monaco-hover:not(.hidden)';
/** A plain `.js` buffer reads as a script unless the editor makes it a module. */
const SCRIPT_PATCH = 'file:///e2e-workspace/patch.js';

test('a guide example hovers like the patch editor', async ({ window }) => {
    await openUntitledBuffer(window);
    await window.getByRole('button', { name: 'Docs' }).click();
    const docs = window.locator('.docs-panel');
    await docs.getByRole('button', { name: 'Guide' }).click();

    const example = docs.locator('.code-example.live').first();
    await expect(example).toBeVisible({ timeout: 20_000 });
    const sine = example
        .locator('.code-example-editor .view-line span span')
        .filter({ hasText: /\$sine/ })
        .first();
    await expect(sine).toBeVisible();

    await expect(async () => {
        await window.mouse.move(0, 0);
        await sine.hover();
        await expect(window.locator(HOVER).first()).toContainText('sine', {
            timeout: 2_500,
        });
    }).toPass({ timeout: 30_000 });

    // The example takes no input.
    const before = await example.locator('.view-lines').innerText();
    await sine.click();
    await window.keyboard.type('xyz');
    expect(await example.locator('.view-lines').innerText()).toBe(before);
});

test("a patch's diagnostics are its own while examples show", async ({
    window,
}) => {
    await openUntitledBuffer(window);
    // The welcome example binds `tone` and so does this patch; a patch that
    // shared their scope would report it redeclared.
    await window.evaluate((uri) => {
        window.monaco.editor.createModel(
            "const tone = $saw('c3')\ntone.notAMethod()\n",
            'javascript',
            window.monaco.Uri.parse(uri),
        );
    }, SCRIPT_PATCH);
    await window.getByRole('button', { name: 'Docs' }).click();
    const docs = window.locator('.docs-panel');
    await docs.getByRole('button', { name: 'Guide' }).click();
    await expect(docs.locator('.code-example.live').first()).toBeVisible({
        timeout: 20_000,
    });

    const codes = () =>
        window.evaluate((uri) => {
            const { editor, Uri } = window.monaco;
            return editor
                .getModelMarkers({ resource: Uri.parse(uri) })
                .map((m) =>
                    typeof m.code === 'object' ? m.code.value : String(m.code),
                );
        }, SCRIPT_PATCH);
    // Checked afresh with the examples showing: only the patch's own
    // mistakes, as no method of a signal is named `notAMethod`.
    await window.evaluate((uri) => {
        const model = window.monaco.editor.getModel(
            window.monaco.Uri.parse(uri),
        )!;
        model.setValue(`${model.getValue()}tone.notAMethod()\n`);
    }, SCRIPT_PATCH);
    await expect.poll(codes, { timeout: 30_000 }).toEqual(['2339', '2339']);
    // Long enough for a later, different check to land.
    await window.waitForTimeout(1_000);
    expect(await codes()).toEqual(['2339', '2339']);

    const exampleMarkers = await window.evaluate(() => {
        const { editor } = window.monaco;
        return editor
            .getModels()
            .filter((m) =>
                m.uri.toString().startsWith('file:///shoreline-docs/'),
            )
            .flatMap((m) => editor.getModelMarkers({ resource: m.uri }));
    });
    expect(exampleMarkers).toEqual([]);
});
