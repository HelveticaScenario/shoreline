/**
 * E2E tests for the monospace fonts the app ships: each loads from the app's
 * own files, sets every glyph on one width, and reaches the editor when
 * picked in the settings.
 */

import { BUNDLED_FONTS } from '../../src/shared/fonts';
import { test, expect, openUntitledBuffer } from '../fixtures';

const OPEN_SETTINGS = process.platform === 'darwin' ? 'Meta+,' : 'Control+,';

test.describe('bundled fonts', () => {
    test('every bundled font loads a regular and bold monospaced face', async ({
        window,
    }) => {
        const problems = await window.evaluate(async (fonts) => {
            const context = document.createElement('canvas').getContext('2d')!;
            const found: string[] = [];
            for (const font of fonts) {
                for (const weight of [400, 700]) {
                    const spec = `${weight} 40px "${font}"`;
                    const faces = await document.fonts.load(spec, 'iM');
                    context.font = spec;
                    const narrow = context.measureText('iiii').width;
                    const wide = context.measureText('MMMM').width;
                    if (faces.length === 0) {
                        found.push(`${font} ${weight}: no face loaded`);
                    } else if (Math.abs(narrow - wide) > 0.5) {
                        found.push(
                            `${font} ${weight}: iiii ${narrow}px, MMMM ${wide}px`,
                        );
                    }
                }
            }
            return found;
        }, BUNDLED_FONTS);

        expect(problems).toEqual([]);
    });

    test('a font picked in the settings reaches the editor', async ({
        window,
    }) => {
        await openUntitledBuffer(window);
        await window.keyboard.press(OPEN_SETTINGS);
        await window
            .locator('.settings-section')
            .filter({
                has: window.getByRole('heading', { name: 'Font', exact: true }),
            })
            .locator('select')
            .selectOption('0xProto');

        await expect
            .poll(() =>
                window.evaluate(
                    () =>
                        getComputedStyle(
                            document.querySelector(
                                '.monaco-editor .view-lines',
                            )!,
                        ).fontFamily,
                ),
            )
            .toContain('0xProto');
    });
});
