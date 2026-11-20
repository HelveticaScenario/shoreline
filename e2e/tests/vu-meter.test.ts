/**
 * E2E tests for the mixer panel: per-track meters, mute/solo buttons
 * (label prefix edit + live gate update), code-only Ctrl/Cmd edits with ghost
 * state, panel toggling, and clip indication.
 */

import { test, expect, openUntitledBuffer } from '../fixtures';

const TWO_OUT_PATCH = [`lead: $sine($hz(440))`, `bass$0m: $saw($hz(110))`].join(
    '\n',
);

/** Sum of a frame's per-channel rms for the given tap module id. */
async function rmsFor(
    window: import('playwright').Page,
    tapModuleId: string,
): Promise<number> {
    return window.evaluate(async (id) => {
        const frames = await window.__TEST_API__!.getVuMeterData();
        const frame = frames.find((f: any) => f.moduleId === id);
        if (!frame) {
            return 0;
        }
        return frame.rms.reduce((a: number, b: number) => a + b, 0);
    }, tapModuleId);
}

/** A saw at 25 V peak clips; a sine at 19 V peak sits just under 0 dB. */
const CLIP_PATCH = [
    `hot: $saw('c3').mul(5)`,
    `near: $sine('c4').mul(3.8)`,
].join('\n');

/** Whether a meter's bars draw in the clip reds, sampled at −30 dB, a zone
 *  every audible level fills. */
async function barsAreRed(
    window: import('playwright').Page,
    key: string,
): Promise<boolean> {
    return window.evaluate((k) => {
        const canvas = document.querySelector(
            `.vu-meter[data-vu-key="${k}"] canvas`,
        ) as HTMLCanvasElement;
        const ctx = canvas.getContext('2d')!;
        const dpr = window.devicePixelRatio || 1;
        const padY = 5 * dpr;
        // −30 dB on the meter's piecewise scale (−24 dB break at 60%).
        const norm = 0.6 + (6 / 36) * 0.4;
        const y = Math.round(padY + norm * (canvas.height - padY * 2));
        const row = ctx.getImageData(0, y, canvas.width, 1).data;
        let brightest = [0, 0];
        for (let i = 0; i < row.length; i += 4) {
            if (row[i] + row[i + 1] > brightest[0] + brightest[1]) {
                brightest = [row[i], row[i + 1]];
            }
        }
        return brightest[0] > brightest[1];
    }, key);
}

function readout(window: import('playwright').Page, key: string) {
    return window.locator(`.vu-meter[data-vu-key="${key}"] .vu-peak-readout`);
}

/** Run `patch` in a new buffer and wait until the mixer lists `track`. */
async function runPatch(
    window: import('playwright').Page,
    patch: string,
    track: string,
) {
    await openUntitledBuffer(window);
    await window.evaluate((p) => {
        window.__TEST_API__!.setEditorValue(p);
    }, patch);
    await window.evaluate(() => window.__TEST_API__!.executePatch());
    await expect
        .poll(
            () =>
                window.evaluate(() =>
                    window.__TEST_API__!.getVuOutputs().map((o: any) => o.key),
                ),
            { timeout: 15_000 },
        )
        .toContain(track);
}

async function setupClipPatch(window: import('playwright').Page) {
    await window.waitForTimeout(3000);
    const hasTestAPI = await window.evaluate(() => !!window.__TEST_API__);
    test.skip(!hasTestAPI, '__TEST_API__ not available');

    await runPatch(window, CLIP_PATCH, 'hot');
    await window.evaluate(() => {
        window.__TEST_API__!.setMixerVisible(true);
    });
    await expect(readout(window, 'hot')).toBeVisible();
}

async function setupTwoOutPatch(window: import('playwright').Page) {
    await window.waitForTimeout(3000);
    const hasTestAPI = await window.evaluate(() => !!window.__TEST_API__);
    test.skip(!hasTestAPI, '__TEST_API__ not available');

    await runPatch(window, TWO_OUT_PATCH, 'lead');

    await window.evaluate(() => {
        window.__TEST_API__!.setMixerVisible(true);
    });
    await window.waitForTimeout(500);
}

test.describe('vu meter panel', () => {
    test('shows one labeled meter per out', async ({ window }) => {
        await setupTwoOutPatch(window);

        await expect(window.locator('.vu-meter-panel')).toBeVisible();
        // Two channel strips plus the pinned end-of-chain master strip.
        await expect(window.locator('.vu-meter')).toHaveCount(3);
        await expect(window.locator('.vu-meter--main')).toBeVisible();
        await expect(
            window.locator('.vu-meter[data-vu-key="lead"] .vu-meter-label'),
        ).toHaveText('lead');
        await expect(
            window.locator('.vu-meter[data-vu-key="bass"] .vu-meter-label'),
        ).toHaveText('bass');

        // Both outputs report live loudness.
        await expect
            .poll(() => rmsFor(window, '__vuTap_lead'), { timeout: 10_000 })
            .toBeGreaterThan(0.01);
        await expect
            .poll(() => rmsFor(window, '__vuTap_bass'), { timeout: 10_000 })
            .toBeGreaterThan(0.01);
    });

    test('mute edits the source, greys the meter, keeps it metering', async ({
        window,
    }) => {
        await setupTwoOutPatch(window);

        await window
            .locator('.vu-meter[data-vu-key="lead"] .vu-btn-mute')
            .click();
        await window.waitForTimeout(300);

        const source = await window.evaluate(() =>
            window.__TEST_API__!.getEditorValue(),
        );
        expect(source).toContain(`_lead: $sine`);

        await expect(
            window.locator('.vu-meter[data-vu-key="lead"]'),
        ).toHaveClass(/vu-meter--suppressed/);

        // The tap sits before the mute gate, so a muted output keeps
        // metering — that is the feature.
        await window.waitForTimeout(1000);
        expect(await rmsFor(window, '__vuTap_lead')).toBeGreaterThan(0.01);

        // Un-mute removes the property again.
        await window
            .locator('.vu-meter[data-vu-key="lead"] .vu-btn-mute')
            .click();
        await window.waitForTimeout(300);
        const restored = await window.evaluate(() =>
            window.__TEST_API__!.getEditorValue(),
        );
        expect(restored).toContain(`lead: $sine`);
        expect(restored).not.toContain('_lead');
        await expect(
            window.locator('.vu-meter[data-vu-key="lead"]'),
        ).not.toHaveClass(/vu-meter--suppressed/);
    });

    test('solo suppresses the other output and edits the source', async ({
        window,
    }) => {
        await setupTwoOutPatch(window);

        await window
            .locator('.vu-meter[data-vu-key="bass"] .vu-btn-solo')
            .click();
        await window.waitForTimeout(300);

        const source = await window.evaluate(() =>
            window.__TEST_API__!.getEditorValue(),
        );
        expect(source).toContain(`S_bass$0m: $saw`);

        await expect(
            window.locator('.vu-meter[data-vu-key="lead"]'),
        ).toHaveClass(/vu-meter--suppressed/);
        await expect(
            window.locator('.vu-meter[data-vu-key="bass"]'),
        ).not.toHaveClass(/vu-meter--suppressed/);
    });

    test('solo on a muted track keeps its mute and plays it', async ({
        window,
    }) => {
        await setupTwoOutPatch(window);
        const strip = window.locator('.vu-meter[data-vu-key="lead"]');
        await strip.locator('.vu-btn-mute').click();
        await window.waitForTimeout(300);
        await strip.locator('.vu-btn-solo').click();
        await window.waitForTimeout(300);

        const source = await window.evaluate(() =>
            window.__TEST_API__!.getEditorValue(),
        );
        expect(source).toContain(`S__lead: $sine`);
        await expect(strip).not.toHaveClass(/vu-meter--suppressed/);
        await expect(strip.locator('.vu-btn-mute')).toHaveAttribute(
            'aria-pressed',
            'true',
        );
        await expect(strip.locator('.vu-btn-solo')).toHaveAttribute(
            'aria-pressed',
            'true',
        );
    });

    test('mute state derives from the source across re-evals', async ({
        window,
    }) => {
        await setupTwoOutPatch(window);

        await window
            .locator('.vu-meter[data-vu-key="lead"] .vu-btn-mute')
            .click();
        await window.waitForTimeout(300);

        // Re-run the patch: the edited source recompiles to the same state.
        await window.evaluate(() => window.__TEST_API__!.executePatch());
        await window.waitForTimeout(2000);

        const outputs = await window.evaluate(() =>
            window.__TEST_API__!.getVuOutputs(),
        );
        const lead = outputs.find((o: any) => o.key === 'lead');
        expect(lead.mute).toBe(true);
        await expect(
            window.locator('.vu-meter[data-vu-key="lead"]'),
        ).toHaveClass(/vu-meter--suppressed/);
    });

    test('cmd-click mute edits the source only until a patch update applies it', async ({
        window,
    }) => {
        await setupTwoOutPatch(window);

        const muteButton = window.locator(
            '.vu-meter[data-vu-key="lead"] .vu-btn-mute',
        );

        // Code-only gesture: the source gains mute: true...
        await muteButton.click({ modifiers: ['ControlOrMeta'] });
        await window.waitForTimeout(300);
        const source = await window.evaluate(() =>
            window.__TEST_API__!.getEditorValue(),
        );
        expect(source).toContain(`_lead: $sine`);

        // ...but the audio is untouched: the meter stays live and the
        // button's inner (audio) section stays unmuted while the outer
        // (code) section shows the pending mute.
        await expect(
            window.locator('.vu-meter[data-vu-key="lead"]'),
        ).not.toHaveClass(/vu-meter--suppressed/);
        await expect(muteButton).toHaveAttribute('aria-pressed', 'false');
        await expect(muteButton).toHaveAttribute('data-code-pressed', 'true');

        // A patch update compiles the edited source and re-syncs audio to
        // code, clearing the ghost.
        await window.evaluate(() => window.__TEST_API__!.executePatch());
        await window.waitForTimeout(2000);
        await expect(
            window.locator('.vu-meter[data-vu-key="lead"]'),
        ).toHaveClass(/vu-meter--suppressed/);
        await expect(muteButton).toHaveAttribute('aria-pressed', 'true');
        await expect(muteButton).toHaveAttribute('data-code-pressed', 'true');
    });

    test('cmd-click toggles the code state from the ghost, not the audio', async ({
        window,
    }) => {
        await setupTwoOutPatch(window);

        const muteButton = window.locator(
            '.vu-meter[data-vu-key="lead"] .vu-btn-mute',
        );

        // Two code-only clicks cancel out: the second toggles the ghost
        // back, restoring the source and re-syncing the outer section.
        await muteButton.click({ modifiers: ['ControlOrMeta'] });
        await window.waitForTimeout(300);
        await muteButton.click({ modifiers: ['ControlOrMeta'] });
        await window.waitForTimeout(300);

        const source = await window.evaluate(() =>
            window.__TEST_API__!.getEditorValue(),
        );
        expect(source).not.toContain('_lead');
        await expect(muteButton).toHaveAttribute('aria-pressed', 'false');
        await expect(muteButton).toHaveAttribute('data-code-pressed', 'false');
    });

    test('cmd-right-click reverts the code gain to the audio value', async ({
        window,
    }) => {
        await setupTwoOutPatch(window);

        const canvas = window.locator(
            '.vu-meter[data-vu-key="lead"] .vu-meter-canvas',
        );

        // Code-only gain jump: the source gains an explicit gain option
        // while the audio keeps running at unity.
        await canvas.click({
            modifiers: ['ControlOrMeta'],
            position: { x: 20, y: 15 },
        });
        await window.waitForTimeout(300);
        const edited = await window.evaluate(() =>
            window.__TEST_API__!.getEditorValue(),
        );
        expect(edited).toContain('gain:');
        const outputs = await window.evaluate(() =>
            window.__TEST_API__!.getVuOutputs(),
        );
        expect(outputs.find((o: any) => o.key === 'lead').gain).toBe(5);

        // Cmd-right-click reverts the source to the audio's unity gain
        // (the property is removed) and drops the ghost.
        await canvas.click({
            button: 'right',
            modifiers: ['ControlOrMeta'],
            position: { x: 20, y: 15 },
        });
        await window.waitForTimeout(300);
        const reverted = await window.evaluate(() =>
            window.__TEST_API__!.getEditorValue(),
        );
        expect(reverted).not.toContain('gain:');
        expect(reverted).toContain(`lead: $sine($hz(440))\n`);
    });

    test('mixer controls still edit the code while the patch is stopped', async ({
        window,
    }) => {
        await setupTwoOutPatch(window);
        await window.getByRole('button', { name: /Stop/ }).click();
        await window.waitForTimeout(500);

        await window
            .locator('.vu-meter[data-vu-key="lead"] .vu-btn-mute')
            .click();
        await window
            .locator('.vu-meter--main .vu-meter-canvas')
            .click({ position: { x: 20, y: 15 } });
        await window.waitForTimeout(300);
        const edited = await window.evaluate(() =>
            window.__TEST_API__!.getEditorValue(),
        );
        expect(edited).toContain(`_lead: $sine`);
        expect(edited).toMatch(/\$setMainGain\([\d.]+\)/);
    });

    test('the main fader writes $setMainGain into the code', async ({
        window,
    }) => {
        await setupTwoOutPatch(window);
        await window
            .locator('.vu-meter--main .vu-meter-canvas')
            .click({ position: { x: 20, y: 15 } });
        await window.waitForTimeout(300);
        const edited = await window.evaluate(() =>
            window.__TEST_API__!.getEditorValue(),
        );
        expect(edited).toMatch(/\$setMainGain\([\d.]+\)/);
    });

    test('right-click resets the gain to unity in audio and code', async ({
        window,
    }) => {
        await setupTwoOutPatch(window);

        const canvas = window.locator(
            '.vu-meter[data-vu-key="lead"] .vu-meter-canvas',
        );

        // Plain click sets both the audio and the source to the clicked
        // position.
        await canvas.click({ position: { x: 20, y: 15 } });
        await window.waitForTimeout(300);
        const edited = await window.evaluate(() =>
            window.__TEST_API__!.getEditorValue(),
        );
        expect(edited).toContain('gain:');
        let outputs = await window.evaluate(() =>
            window.__TEST_API__!.getVuOutputs(),
        );
        expect(outputs.find((o: any) => o.key === 'lead').gain).not.toBe(5);

        // Plain right-click resets both back to unity; the source drops
        // the property.
        await canvas.click({
            button: 'right',
            position: { x: 20, y: 15 },
        });
        await window.waitForTimeout(300);
        const reset = await window.evaluate(() =>
            window.__TEST_API__!.getEditorValue(),
        );
        expect(reset).not.toContain('gain:');
        outputs = await window.evaluate(() =>
            window.__TEST_API__!.getVuOutputs(),
        );
        expect(outputs.find((o: any) => o.key === 'lead').gain).toBe(5);
    });

    test('the fader writes a .track() again after a reset drops it', async ({
        window,
    }) => {
        await window.waitForTimeout(3000);
        const hasTestAPI = await window.evaluate(() => !!window.__TEST_API__);
        test.skip(!hasTestAPI, '__TEST_API__ not available');

        await runPatch(
            window,
            `lead: $sine($hz(440)).track({ gain: 3 })`,
            'lead',
        );
        await window.evaluate(() => {
            window.__TEST_API__!.setMixerVisible(true);
        });
        await window.waitForTimeout(500);

        const canvas = window.locator(
            '.vu-meter[data-vu-key="lead"] .vu-meter-canvas',
        );
        await canvas.click({ button: 'right', position: { x: 20, y: 15 } });
        await window.waitForTimeout(300);
        expect(
            await window.evaluate(() => window.__TEST_API__!.getEditorValue()),
        ).toBe(`lead: $sine($hz(440))`);

        await canvas.click({ position: { x: 20, y: 15 } });
        await window.waitForTimeout(300);
        expect(
            await window.evaluate(() => window.__TEST_API__!.getEditorValue()),
        ).toMatch(/^lead: \$sine\(\$hz\(440\)\)\.track\(\{ gain: [\d.]+ \}\)$/);
    });

    test('a bus track meters what is sent to it', async ({
        window,
    }, testInfo) => {
        await window.waitForTimeout(3000);
        const hasTestAPI = await window.evaluate(() => !!window.__TEST_API__);
        test.skip(!hasTestAPI, '__TEST_API__ not available');

        await runPatch(
            window,
            [
                `dry: $sine($hz(440)).track({ sends: [fx, 5] })`,
                `fx$2: $bus().lpf('c8')`,
            ].join('\n'),
            'dry',
        );
        await window.evaluate(() => {
            window.__TEST_API__!.setMixerVisible(true);
        });
        // A bus strip sits just left of the main strip, lettered like a
        // return.
        const bus = window.locator(
            '.vu-meter-buses:has(+ .vu-meter--main) > .vu-meter[data-vu-key="fx"]',
        );
        await expect(bus).toHaveCount(1);
        await expect(bus.locator('.vu-btn-mute')).toHaveText('A');
        // A bus has no solo button, only the gap where one would be.
        await expect(bus.locator('.vu-btn-solo')).toHaveCount(0);
        await expect(bus.locator('.vu-btn-gap')).toHaveCount(1);
        await expect(
            window.locator('.vu-meter-scroll .vu-meter[data-vu-key="dry"]'),
        ).toHaveCount(1);
        await window.screenshot({ path: testInfo.outputPath('buses.png') });
        await expect
            .poll(() => rmsFor(window, '__vuTap_fx'), { timeout: 15_000 })
            .toBeGreaterThan(0.01);
    });

    test('a group shows its tracks beside it and folds them away', async ({
        window,
    }, testInfo) => {
        await window.waitForTimeout(3000);
        const hasTestAPI = await window.evaluate(() => !!window.__TEST_API__);
        test.skip(!hasTestAPI, '__TEST_API__ not available');

        await runPatch(
            window,
            [
                `kick$drums: $sine($hz(55))`,
                `hats$drums: $noise()`,
                `drums$rhythm: $group().lpf('c8')`,
                `bass$rhythm: $saw($hz(110))`,
                `rhythm: $group()`,
                `lead: $sine($hz(440))`,
            ].join('\n'),
            'lead',
        );
        await window.evaluate(() => {
            window.__TEST_API__!.setMixerVisible(true);
        });

        const drums = window.locator('.vu-group[data-vu-group="drums"]');
        await expect(drums.locator('.vu-meter')).toHaveCount(3);
        await expect(
            window.locator(
                '.vu-group[data-vu-group="rhythm"] > .vu-group[data-vu-group="drums"]',
            ),
        ).toHaveCount(1);
        await expect(
            window.locator('.vu-meter-scroll > .vu-meter[data-vu-key="lead"]'),
        ).toHaveCount(1);
        await window.screenshot({ path: testInfo.outputPath('groups.png') });

        await drums.locator('.vu-group-fold').first().click();
        await expect(drums.locator('.vu-meter')).toHaveCount(1);
        await expect(
            window.locator('.vu-meter[data-vu-key="kick"]'),
        ).toHaveCount(0);
        await window.screenshot({ path: testInfo.outputPath('folded.png') });
        await drums.locator('.vu-group-fold').first().click();
        await expect(drums.locator('.vu-meter')).toHaveCount(3);
    });

    test('panel visibility toggles', async ({ window }) => {
        await setupTwoOutPatch(window);
        await expect(window.locator('.vu-meter-panel')).toBeVisible();

        await window.evaluate(() => {
            window.__TEST_API__!.setMixerVisible(false);
        });
        await expect(window.locator('.vu-meter-panel')).not.toBeVisible();
    });

    test('placeholder shows when the patch has no outs', async ({ window }) => {
        await window.waitForTimeout(3000);
        const hasTestAPI = await window.evaluate(() => !!window.__TEST_API__);
        test.skip(!hasTestAPI, '__TEST_API__ not available');

        await openUntitledBuffer(window);
        await window.evaluate(() => {
            window.__TEST_API__!.setEditorValue('$sine($hz(440))');
        });
        await window.evaluate(() => window.__TEST_API__!.executePatch());
        await window.waitForTimeout(1500);
        await window.evaluate(() => {
            window.__TEST_API__!.setMixerVisible(true);
        });

        await expect(window.locator('.vu-meter-empty')).toBeVisible();
    });

    test('a clipping track turns its bars and readout red', async ({
        window,
    }) => {
        await setupClipPatch(window);

        await expect(readout(window, 'hot')).toHaveAttribute(
            'data-clipped',
            'true',
        );
        expect(
            Number(await readout(window, 'hot').textContent()),
        ).toBeGreaterThan(0);
        expect(await barsAreRed(window, 'hot')).toBe(true);
        // Just under 0 dB never reads as clipping.
        await expect(readout(window, 'near')).toHaveAttribute(
            'data-clipped',
            'false',
        );
        expect(await barsAreRed(window, 'near')).toBe(false);
    });

    test('bars recover after the hold while the readout keeps the clip until clicked', async ({
        window,
    }) => {
        await setupClipPatch(window);
        await expect(readout(window, 'hot')).toHaveAttribute(
            'data-clipped',
            'true',
        );

        // Pull the fader to about −36 dB.
        const canvas = window.locator('.vu-meter[data-vu-key="hot"] canvas');
        const box = (await canvas.boundingBox())!;
        await window.mouse.click(
            box.x + box.width / 2,
            box.y + box.height * 0.75,
        );

        // The 1.5 s peak hold, then the release below 0 dB.
        await expect
            .poll(() => barsAreRed(window, 'hot'), { timeout: 10_000 })
            .toBe(false);
        await expect(readout(window, 'hot')).toHaveAttribute(
            'data-clipped',
            'true',
        );

        await readout(window, 'hot').click();
        await expect(readout(window, 'hot')).toHaveAttribute(
            'data-clipped',
            'false',
        );
        await expect
            .poll(async () =>
                Number(await readout(window, 'hot').textContent()),
            )
            .toBeLessThan(-20);
    });

    test('the readout keeps a clip while the mixer is closed', async ({
        window,
    }) => {
        await setupClipPatch(window);
        await expect(readout(window, 'hot')).toHaveAttribute(
            'data-clipped',
            'true',
        );

        await window.evaluate(() => {
            window.__TEST_API__!.setMixerVisible(false);
        });
        await expect(window.locator('.vu-meter-panel')).not.toBeVisible();
        await window.evaluate(() => {
            window.__TEST_API__!.setMixerVisible(true);
        });
        await expect(readout(window, 'hot')).toHaveAttribute(
            'data-clipped',
            'true',
        );
    });

    test('a clip made while the mixer is closed shows when it opens', async ({
        window,
    }) => {
        await window.waitForTimeout(3000);
        const hasTestAPI = await window.evaluate(() => !!window.__TEST_API__);
        test.skip(!hasTestAPI, '__TEST_API__ not available');

        await window.evaluate(() => {
            window.__TEST_API__!.setMixerVisible(false);
        });
        // Clips for the first 0.16 s of each 16 s cycle, then plays quietly.
        await runPatch(
            window,
            `hot: $saw('c3').mul($pulse('0.0625hz', { width: 0.05 }).range(0.2, 5))`,
            'hot',
        );
        await window.waitForTimeout(1000);

        await window.evaluate(() => {
            window.__TEST_API__!.setMixerVisible(true);
        });
        await expect(readout(window, 'hot')).toHaveAttribute(
            'data-clipped',
            'true',
        );
    });

    test('stopping drops the bars to silence and keeps the readout', async ({
        window,
    }) => {
        await setupClipPatch(window);
        await expect(readout(window, 'hot')).toHaveAttribute(
            'data-clipped',
            'true',
        );

        await window.keyboard.press('Control+.');
        await expect
            .poll(() =>
                window.evaluate(() => window.__TEST_API__!.isClockRunning()),
            )
            .toBe(false);
        expect(await barsAreRed(window, 'hot')).toBe(false);
        await expect(readout(window, 'hot')).toHaveAttribute(
            'data-clipped',
            'true',
        );
    });
});
