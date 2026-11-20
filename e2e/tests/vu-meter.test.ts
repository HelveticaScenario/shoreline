/**
 * E2E tests for the mixer panel: per-track meters, mute/solo buttons
 * (label prefix edit + live gate update), code-only Ctrl/Cmd edits with ghost
 * state, and panel toggling.
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

/**
 * Play `lines` in a new buffer with the mixer showing, until the mixer shows
 * track `key`: a run that starts before the buffer holds the patch plays
 * what the buffer held, so the patch runs again until it is the one playing.
 * Skips the test when the app exposes no test API.
 */
async function playPatch(
    window: import('playwright').Page,
    lines: string[],
    key: string,
) {
    await window.waitForTimeout(3000);
    const hasTestAPI = await window.evaluate(() => !!window.__TEST_API__);
    test.skip(!hasTestAPI, '__TEST_API__ not available');

    await openUntitledBuffer(window);
    await window.evaluate((patch) => {
        window.__TEST_API__!.setEditorValue(patch);
        window.__TEST_API__!.setMixerVisible(true);
    }, lines.join('\n'));
    await expect
        .poll(
            async () => {
                await window.evaluate(() =>
                    window.__TEST_API__!.executePatch(),
                );
                return window
                    .locator(`.vu-meter[data-vu-key="${key}"]`)
                    .count();
            },
            { timeout: 15_000 },
        )
        .toBeGreaterThan(0);
}

async function setupTwoOutPatch(window: import('playwright').Page) {
    await playPatch(window, TWO_OUT_PATCH.split('\n'), 'bass');
    await window.waitForTimeout(2000);
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
        expect(outputs.find((o: any) => o.key === 'lead').gain.value).toBe(5);

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
        expect(outputs.find((o: any) => o.key === 'lead').gain.value).not.toBe(
            5,
        );

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
        expect(outputs.find((o: any) => o.key === 'lead').gain.value).toBe(5);
    });

    test('the fader writes a .track() again after a reset drops it', async ({
        window,
    }) => {
        await window.waitForTimeout(3000);
        const hasTestAPI = await window.evaluate(() => !!window.__TEST_API__);
        test.skip(!hasTestAPI, '__TEST_API__ not available');

        await openUntitledBuffer(window);
        await window.evaluate(() => {
            window.__TEST_API__!.setEditorValue(
                `lead: $sine($hz(440)).track({ gain: 3 })`,
            );
        });
        await window.evaluate(() => window.__TEST_API__!.executePatch());
        await window.waitForFunction(
            () => window.__TEST_API__!.getLastPatchResult() !== null,
        );
        await window.waitForTimeout(1000);
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
        await playPatch(
            window,
            [
                `dry: $sine($hz(440)).track({ sends: [fx, 5] })`,
                `fx$2: $bus().lpf('c8')`,
            ],
            'fx',
        );
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

    test('send dials add, set and remove a send to a bus', async ({
        window,
    }, testInfo) => {
        await playPatch(
            window,
            [
                `lead: $sine($hz(440))`,
                `pad: $saw($hz(220)).track({ sends: [verb, $sine(1).range(0, 5)] })`,
                `fx$2: $bus().lpf('c8')`,
                `verb$4: $bus()`,
            ],
            'verb',
        );

        // One dial per bus on each track, lettered like the buses; none on
        // the buses or the main.
        const lead = window.locator('.vu-meter[data-vu-key="lead"]');
        await expect(lead.locator('.vu-send')).toHaveCount(2);
        await expect(lead.locator('.vu-send-letter')).toHaveText(['A', 'B']);
        await expect(
            window.locator('.vu-meter[data-vu-key="fx"] .vu-send'),
        ).toHaveCount(0);
        await expect(window.locator('.vu-meter--main .vu-send')).toHaveCount(0);
        // A signal-driven send is locked.
        await expect(
            window
                .locator('.vu-meter[data-vu-key="pad"] .vu-send .vu-knob')
                .nth(1),
        ).toHaveClass(/vu-knob--locked/);
        await expect
            .poll(() => rmsFor(window, '__vuTap_fx'))
            .toBeLessThan(0.001);

        // Dragging up adds the send and plays it at once.
        const dial = lead.locator('.vu-send .vu-knob').first();
        const box = (await dial.boundingBox())!;
        const x = box.x + box.width / 2;
        const y = box.y + box.height / 2;
        const source = () =>
            window.evaluate(() => window.__TEST_API__!.getEditorValue());
        await window.mouse.move(x, y);
        await window.mouse.down();
        await window.mouse.move(x, y - 20, { steps: 5 });
        await window.mouse.move(x, y - 40, { steps: 5 });
        await expect
            .poll(() => rmsFor(window, '__vuTap_fx'), { timeout: 10_000 })
            .toBeGreaterThan(0.01);
        await window.mouse.up();
        await expect
            .poll(source)
            .toContain(`lead: $sine($hz(440)).track({ sends: [fx, 2] })`);
        await window.screenshot({ path: testInfo.outputPath('sends.png') });

        // Released at 0, the send and then the `.track()` go.
        await window.mouse.move(x, y);
        await window.mouse.down();
        await window.mouse.move(x, y + 60, { steps: 5 });
        await window.mouse.up();
        await expect.poll(source).toMatch(/^lead: \$sine\(\$hz\(440\)\)$/m);
        await expect
            .poll(() => rmsFor(window, '__vuTap_fx'), { timeout: 10_000 })
            .toBeLessThan(0.001);
    });

    test('cmd-drag on a send dial edits the code only until reverted', async ({
        window,
    }) => {
        await playPatch(
            window,
            [`lead: $sine($hz(440))`, `fx$2: $bus()`],
            'fx',
        );
        const dial = window
            .locator('.vu-meter[data-vu-key="lead"] .vu-send .vu-knob')
            .first();
        await expect(dial).toBeVisible();
        const box = (await dial.boundingBox())!;
        const x = box.x + box.width / 2;
        const y = box.y + box.height / 2;
        const source = () =>
            window.evaluate(() => window.__TEST_API__!.getEditorValue());

        await window.keyboard.down('Meta');
        await window.mouse.move(x, y);
        await window.mouse.down();
        await window.mouse.move(x, y - 40, { steps: 10 });
        await window.mouse.up();
        await window.keyboard.up('Meta');

        // The code gains the send; the audio keeps sending nothing, and the
        // dial shows the code-side level as a ghost.
        await expect
            .poll(source)
            .toContain(`lead: $sine($hz(440)).track({ sends: [fx, 2] })`);
        await expect(dial.locator('.vu-knob-ghost')).toHaveCount(1);
        await window.waitForTimeout(1000);
        expect(await rmsFor(window, '__vuTap_fx')).toBeLessThan(0.001);

        // Cmd-right-click puts the code back to what is playing.
        await dial.click({ button: 'right', modifiers: ['Meta'] });
        await expect.poll(source).toMatch(/^lead: \$sine\(\$hz\(440\)\)$/m);
        await expect(dial.locator('.vu-knob-ghost')).toHaveCount(0);
    });

    test('the sends hide together when the mixer is too short', async ({
        window,
    }) => {
        const buses = 'abcdefghijkl'.split('');
        await playPatch(
            window,
            [
                `lead: $sine($hz(440))`,
                ...buses.map((n, i) => `${n}$${2 + 2 * i}: $bus()`),
            ],
            'l',
        );
        // Four rows of dials need more height than the panel has: none show.
        await expect(window.locator('.vu-sends')).toHaveCount(0);
    });

    test('a group shows its tracks beside it and folds them away', async ({
        window,
    }, testInfo) => {
        await playPatch(
            window,
            [
                `kick$drums: $sine($hz(55))`,
                `hats$drums: $noise()`,
                `drums$rhythm: $group().lpf('c8')`,
                `bass$rhythm: $saw($hz(110))`,
                `rhythm: $group()`,
                `lead: $sine($hz(440))`,
            ],
            'lead',
        );

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
});
