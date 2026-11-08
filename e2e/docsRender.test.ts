/**
 * Renders every module's docs in the real editor and Reference pane and
 * checks the rendered text: no raw markdown, no `undefined`/`NaN`, every
 * declared range shown, and no horizontal overflow at any sidebar width.
 *
 * Set DOCS_RENDER_SHOTS=<dir> to also save a screenshot of every hover,
 * completion detail, signature help and Reference card.
 */

import * as fs from 'fs';
import * as path from 'path';
import type { Page } from '@playwright/test';
import schemas from '../crates/modular/schemas.json';
import { test, expect, openUntitledBuffer } from './fixtures';
import {
    formatOutputMeta,
    paramMeta,
} from '../src/shared/dsl/paramRange';
import {
    dollarMethodName,
    processSchemas,
    qualifiesForDollarChain,
} from '../src/main/dsl/paramsSchema';

type Schema = (typeof schemas)[number];

const SHOTS = process.env.DOCS_RENDER_SHOTS;
const MODULES = schemas.filter(
    (m) => !m.name.startsWith('_') && m.name !== '$buffer',
);
const CHAINABLE = new Set(
    processSchemas(schemas as never)
        .filter(qualifiesForDollarChain)
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

/** The chain path after `.$.`, e.g. `unstable.filter.lp` for `$unstable.filter.lp`. */
function chainPath(name: string): string {
    const parts = name.split('.');
    parts[0] = parts[0].replace(/^\$/, '');
    parts[parts.length - 1] = dollarMethodName(parts[parts.length - 1]);
    return parts.join('.');
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
    return page.locator(selector).first().evaluate((el) => {
        const clone = el.cloneNode(true) as HTMLElement;
        clone.querySelectorAll('pre, .monaco-tokenized-source').forEach(
            (n) => n.remove(),
        );
        return clone.innerText;
    });
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
            await trigger(window, 'editor.action.showHover');
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

            // Hover on the `.$.` and `.$m.` chain forms.
            if (CHAINABLE.has(name)) {
                for (const via of ['$', '$m']) {
                    const code = `$saw('c3').${via}.${chainPath(name)}()`;
                    await setCode(window, code, code.length - 2);
                    await trigger(window, 'editor.action.showHover');
                    const text = await captureWidget(
                        window,
                        HOVER,
                        'Chain through',
                        shotPath(`hover${via}`, shotName),
                    );
                    check(`${name} .${via}. hover`, text);
                    await window.keyboard.press('Escape');
                }
            }

            // Completion details.
            await setCode(window, name, name.length + 1);
            await trigger(window, 'editor.action.triggerSuggest');
            await expect(
                window.locator('.suggest-widget.visible'),
            ).toBeVisible({ timeout: 20_000 });
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
                const code = `${name}(${', '.repeat(i)}`;
                await setCode(window, code, code.length + 1);
                await trigger(window, 'editor.action.triggerParameterHints');
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
                    failures.push(`${name} signature ${param}: missing (${meta})`);
                }
                await window.keyboard.press('Escape');
            }
        }

        expect(failures, failures.join('\n')).toEqual([]);
    });

    for (const [label, width] of [
        ['min', 160],
        ['default', null],
        ['wide', 900],
    ] as const) {
        test(`reference cards at ${label} width`, async ({ window }) => {
            await window.evaluate((width) => {
                if (width === null) {
                    localStorage.removeItem('modular_sidebar_width');
                } else {
                    localStorage.setItem('modular_sidebar_width', String(width));
                }
            }, width);
            await window.setViewportSize({ height: 1000, width: 1600 });
            await window.reload();
            await window.waitForLoadState('networkidle');
            await window.getByRole('button', { name: 'Docs' }).click();
            const docs = window.locator('.docs-panel');
            await docs.getByRole('button', { name: 'Reference' }).click();
            const search = docs.locator('input[type="search"], input').first();

            const failures: string[] = [];
            for (const module of MODULES) {
                await search.fill(module.name);
                const card = docs
                    .locator('.module-card')
                    .filter({
                        has: window.locator('h3 code', {
                            hasText: `${module.name}(`,
                        }),
                    })
                    .first();
                await expect(card).toBeVisible({ timeout: 10_000 });
                const text = await card.evaluate((el) => {
                    const clone = el.cloneNode(true) as HTMLElement;
                    clone.querySelectorAll('pre').forEach((n) => n.remove());
                    return clone.innerText;
                });
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
                    failures.push(
                        `${module.name}: overflows by ${overflow}px`,
                    );
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
