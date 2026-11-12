/**
 * Renders every module's docs in the real editor and Reference pane and
 * checks the rendered text: no raw markdown, no `undefined`/`NaN`, every
 * declared range shown, and no horizontal overflow at any panel width.
 *
 * Set DOCS_RENDER_SHOTS=<dir> to also save a screenshot of every hover,
 * completion detail, signature help and Reference card.
 */

import * as fs from 'fs';
import * as path from 'path';
import type { Page } from '@playwright/test';
import schemas from '../crates/modular/schemas.json';
import { test, expect, openUntitledBuffer } from './fixtures';
import { formatOutputMeta, paramMeta } from '../src/shared/dsl/paramRange';
import {
    processSchemas,
    qualifiesForChain,
} from '../src/main/dsl/paramsSchema';

type Schema = (typeof schemas)[number];

const SHOTS = process.env.DOCS_RENDER_SHOTS;
const MODULES = schemas.filter(
    (m) => !m.name.startsWith('_') && m.name !== '$buffer',
);
const CHAINABLE = new Set(
    processSchemas(schemas as never)
        .filter(qualifiesForChain)
        .map((s) => s.name),
);

/** Text that means markdown reached the reader unrendered. */
const LEAKS: [string, RegExp][] = [
    ['table rule', /\|\s*:?-{3,}/],
    ['table row', /^\s*\|.*\|\s*$/m],
    ['heading', /(^|\n)\s*#{1,6}\s/],
    ['code fence', /```/],
    ['bold marker', /\*\*/],
    ['raw link tag', /\{@link/],
    ['undefined', /\bundefined\b/],
    ['NaN', /\bNaN\b/],
    ['object', /\[object /],
];

/** The chain path after `.`, e.g. `unstable.filter.lp` for `$unstable.filter.lp`. */
function chainPath(name: string): string {
    return name.replace(/^\$/, '');
}

function leaks(text: string): string[] {
    return LEAKS.filter(([, re]) => re.test(text)).map(([what]) => what);
}

function shotPath(...parts: string[]): string | undefined {
    if (!SHOTS) {
        return undefined;
    }
    const file = path.join(SHOTS, ...parts) + '.png';
    fs.mkdirSync(path.dirname(file), { recursive: true });
    return file;
}

const safe = (name: string) => name.replace(/[^\w.-]+/g, '_');

/** Put `code` in the patch editor with the caret at `column` (1-based). */
async function setCode(page: Page, code: string, column: number) {
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

async function trigger(page: Page, action: string) {
    await page.evaluate((action) => {
        const ed = window.monaco.editor
            .getEditors()
            .find((e) => e.getModel()?.getLanguageId() === 'javascript')!;
        ed.trigger('e2e', action, {});
    }, action);
}

/**
 * The visible widget's rendered text, minus code blocks: the signature and
 * examples are code, where `|` and `undefined` are legitimate.
 */
async function widgetText(page: Page, selector: string): Promise<string> {
    return page
        .locator(selector)
        .first()
        .evaluate((el) => {
            const clone = el.cloneNode(true) as HTMLElement;
            clone
                .querySelectorAll('pre, .monaco-tokenized-source')
                .forEach((n) => n.remove());
            return clone.innerText;
        });
}

/**
 * Run `action` until `selector` shows: the TS worker can miss a request made
 * while it is still processing the previous edit.
 */
async function triggerUntilVisible(
    page: Page,
    action: string,
    selector: string,
) {
    const widget = page.locator(selector).first();
    for (let attempt = 0; attempt < 8; attempt++) {
        await trigger(page, action);
        try {
            await expect(widget).toBeVisible({ timeout: 2_500 });
            return;
        } catch {
            // Retry below.
        }
    }
    await expect(widget).toBeVisible();
}

async function captureWidget(
    page: Page,
    selector: string,
    expectText: string,
    shot: string | undefined,
): Promise<string> {
    const widget = page.locator(selector).first();
    await expect(widget).toBeVisible({ timeout: 20_000 });
    await expect(widget).toContainText(expectText, { timeout: 20_000 });
    if (shot) {
        // Let the widget finish fading in.
        await page.waitForTimeout(300);
        await widget.screenshot({ path: shot });
    }
    return widgetText(page, selector);
}

const HOVER = '.monaco-hover:not(.hidden)';
const SUGGEST_DETAILS = '.suggest-details-container .suggest-details';
const SIGNATURE = '.parameter-hints-widget.visible';

function expectedMetas(module: Schema): string[] {
    const metas: string[] = [];
    for (const name of Object.keys(module.paramsSchema.properties ?? {})) {
        const meta = paramMeta(module, name);
        if (meta) {
            metas.push(`(${meta})`);
        }
    }
    return metas;
}

test.describe('docs render', () => {
    test.setTimeout(60 * 60_000);

    test('editor hover, completion and signature help', async ({ window }) => {
        await openUntitledBuffer(window);
        if (SHOTS) {
            // Screenshots show each widget's whole content, not its
            // scrolled first screen.
            await window.addStyleTag({
                content:
                    '.monaco-hover, .monaco-hover-content, .monaco-hover .monaco-scrollable-element, .suggest-details, .suggest-details .monaco-scrollable-element, .parameter-hints-widget, .parameter-hints-widget .monaco-scrollable-element { max-height: none !important; height: auto !important; }',
            });
        }
        const failures: string[] = [];
        const check = (where: string, text: string) => {
            const found = leaks(text);
            if (found.length > 0) {
                failures.push(`${where}: ${found.join(', ')}`);
            }
        };

        for (const module of MODULES) {
            const name = module.name;
            const shotName = safe(name);
            const metas = expectedMetas(module);
            const tail = name.split('.').pop()!;

            // Hover on the factory.
            await setCode(window, `${name}()`, name.length);
            await triggerUntilVisible(window, 'editor.action.showHover', HOVER);
            const hover = await captureWidget(
                window,
                HOVER,
                tail,
                shotPath('hover', shotName),
            );
            check(`${name} hover`, hover);
            const main =
                module.outputs.find((o) => o.default) ?? module.outputs[0];
            const outMeta = main && formatOutputMeta(main);
            for (const meta of [
                ...metas,
                ...(outMeta ? [`(${outMeta})`] : []),
            ]) {
                if (!hover.includes(meta)) {
                    failures.push(`${name} hover: missing ${meta}`);
                }
            }
            await window.keyboard.press('Escape');

            // Hover on the chain form.
            if (CHAINABLE.has(name)) {
                const code = `$saw('c3').${chainPath(name)}()`;
                await setCode(window, code, code.length - 2);
                await triggerUntilVisible(
                    window,
                    'editor.action.showHover',
                    HOVER,
                );
                const text = await captureWidget(
                    window,
                    HOVER,
                    'Chain through',
                    shotPath('hoverChain', shotName),
                );
                check(`${name} chain hover`, text);
                await window.keyboard.press('Escape');
            }

            // Completion details.
            await setCode(window, name, name.length + 1);
            await triggerUntilVisible(
                window,
                'editor.action.triggerSuggest',
                '.suggest-widget.visible',
            );
            const detailsVisible = await window
                .locator(SUGGEST_DETAILS)
                .isVisible();
            if (!detailsVisible) {
                await trigger(window, 'toggleSuggestionDetails');
            }
            const details = await captureWidget(
                window,
                SUGGEST_DETAILS,
                tail,
                shotPath('completion', shotName),
            );
            check(`${name} completion`, details);
            await window.keyboard.press('Escape');

            // Signature help for each positional, then the config object.
            const positionals = module.positionalArgs.length;
            for (let i = 0; i <= positionals; i++) {
                // Placeholder arguments: a run of empty ones stops TS
                // offering signature help.
                const code = `${name}(${'0, '.repeat(i)}`;
                await setCode(window, code, code.length + 1);
                await triggerUntilVisible(
                    window,
                    'editor.action.triggerParameterHints',
                    SIGNATURE,
                );
                const param =
                    i < positionals ? module.positionalArgs[i].name : 'config';
                const text = await captureWidget(
                    window,
                    SIGNATURE,
                    param,
                    shotPath('signature', `${shotName}-${i}-${param}`),
                );
                check(`${name} signature ${param}`, text);
                const meta =
                    i < positionals ? paramMeta(module, param) : undefined;
                if (meta && !text.includes(`(${meta})`)) {
                    failures.push(
                        `${name} signature ${param}: missing (${meta})`,
                    );
                }
                await window.keyboard.press('Escape');
            }
        }

        expect(failures, failures.join('\n')).toEqual([]);
    });

    // At the bottom the docs share the editor column, so a wider sidebar
    // narrows them; in the sidebar they take its width.
    for (const { label, docsLocation, sidebarWidth, wide } of [
        {
            label: 'wide',
            docsLocation: 'bottom',
            sidebarWidth: 160,
            wide: true,
        },
        {
            label: 'narrow',
            docsLocation: 'bottom',
            sidebarWidth: 1320,
            wide: false,
        },
        {
            label: 'sidebar',
            docsLocation: 'sidebar',
            sidebarWidth: 160,
            wide: false,
        },
    ] as const) {
        test(`reference cards at ${label} width`, async ({ window }) => {
            await window.evaluate(
                async ({ location, width }) => {
                    await window.electronAPI.config.write({
                        docsLocation: location,
                    });
                    localStorage.setItem(
                        'modular_sidebar_width',
                        String(width),
                    );
                },
                { location: docsLocation, width: sidebarWidth },
            );
            await window.setViewportSize({ height: 1000, width: 1600 });
            await window.reload();
            await window.waitForLoadState('networkidle');
            await window.getByRole('button', { name: 'Docs' }).click();
            const docs = window.locator('.docs-panel');
            await docs.getByRole('button', { name: 'Reference' }).click();
            const search = docs.locator('input[type="search"], input').first();

            const failures: string[] = [];
            for (const module of MODULES) {
                await search.fill('');
                await search.fill(module.name);
                const card = docs
                    .locator('.module-card')
                    .filter({
                        has: window.locator('h3 code', {
                            hasText: `${module.name}(`,
                        }),
                    })
                    .first();
                // Exact name matches lead the results; bring the list to its
                // top so the virtualizer mounts that card.
                await expect(async () => {
                    await docs.evaluate((el) => {
                        el.scrollTop = 0;
                        el.querySelectorAll('*').forEach((n) => {
                            if (n.scrollTop > 0) n.scrollTop = 0;
                        });
                    });
                    await expect(card).toBeVisible({ timeout: 1_000 });
                }).toPass({ timeout: 15_000 });
                const text = await card.evaluate((el) => {
                    const clone = el.cloneNode(true) as HTMLElement;
                    clone.querySelectorAll('pre').forEach((n) => n.remove());
                    return clone.innerText;
                });
                if (!text.includes(module.docs.summary)) {
                    failures.push(`${module.name}: summary missing`);
                }
                const examples =
                    module.docs.examples.length +
                    module.docs.sections.reduce(
                        (n, s) => n + s.examples.length,
                        0,
                    );
                if ((await card.locator('pre').count()) !== examples) {
                    failures.push(`${module.name}: examples not all shown`);
                }
                // Each example shows its source exactly, in editor colours.
                const colored = await expect
                    .poll(() =>
                        card.evaluate((el) =>
                            [...el.querySelectorAll('pre')].every((pre) =>
                                pre.querySelector('[class^="mtk"]'),
                            ),
                        ),
                    )
                    .toBe(true)
                    .then(
                        () => true,
                        () => false,
                    );
                if (!colored) {
                    failures.push(`${module.name}: examples not colourized`);
                }
                const codes = [
                    ...module.docs.examples,
                    ...module.docs.sections.flatMap((s) => s.examples),
                ].map((e) => e.code);
                const texts = await card.evaluate((el) =>
                    [...el.querySelectorAll('pre')].map((pre) =>
                        pre.innerText.trimEnd(),
                    ),
                );
                for (const code of codes) {
                    if (!texts.includes(code)) {
                        failures.push(
                            `${module.name}: example text differs from its source`,
                        );
                    }
                }
                const tables = module.docs.sections.filter((s) => s.table);
                if (
                    (await card.locator('table.doc-table').count()) !==
                    tables.length
                ) {
                    failures.push(`${module.name}: tables not all shown`);
                }
                if (tables.length > 0) {
                    // Wide cards show the header row; narrow ones stack rows
                    // under their keys instead.
                    const headerShown = await card
                        .locator('table.doc-table thead')
                        .first()
                        .isVisible();
                    if (headerShown !== wide) {
                        failures.push(
                            `${module.name}: table ${headerShown ? 'not stacked' : 'stacked'} at ${label} width`,
                        );
                    }
                }
                const found = leaks(text);
                if (found.length > 0) {
                    failures.push(`${module.name}: ${found.join(', ')}`);
                }
                for (const meta of expectedMetas(module)) {
                    const bare = meta.slice(1, -1);
                    if (!text.includes(bare)) {
                        failures.push(`${module.name}: missing ${bare}`);
                    }
                }
                const overflow = await card.evaluate(
                    (el) => el.scrollWidth - el.clientWidth,
                );
                if (overflow > 1) {
                    failures.push(`${module.name}: overflows by ${overflow}px`);
                }
                const shot = shotPath(`reference-${label}`, safe(module.name));
                if (shot) {
                    await card.screenshot({ path: shot });
                }
            }
            expect(failures, failures.join('\n')).toEqual([]);
        });
    }
});
