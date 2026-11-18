import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import { BUNDLED_FONTS } from './fonts';

const root = join(__dirname, '..', '..');
const fontsDir = join(root, 'src/renderer/assets/fonts');
const require = createRequire(join(root, 'package.json'));

const stripComments = (css: string) => css.replace(/\/\*[\s\S]*?\*\//g, '');

const declaredFamilies = (css: string) =>
    [...css.matchAll(/font-family:\s*['"]([^'"]+)['"]/g)].map((m) => m[1]);

describe('bundled fonts', () => {
    it('every stylesheet puts its @imports before any rule, where CSS honours them', () => {
        const sheets = readdirSync(join(root, 'src'), { recursive: true })
            .map((file) => String(file).replaceAll('\\', '/'))
            .filter((file) => file.endsWith('.css'));
        expect(sheets).toContain('renderer/index.css');

        const late = sheets.filter((file) => {
            const css = stripComments(
                readFileSync(join(root, 'src', file), 'utf8'),
            );
            const firstRule = css.indexOf('{');
            return (
                firstRule >= 0 &&
                [...css.matchAll(/@import\b/g)].some((m) => m.index > firstRule)
            );
        });
        expect(late).toEqual([]);
    });

    it('the app stylesheet imports the font sheet', () => {
        const css = stripComments(
            readFileSync(join(root, 'src/renderer/index.css'), 'utf8'),
        );
        expect(css).toContain("@import './assets/fonts/fonts.css';");
    });

    it('ships a regular and bold face for every bundled font', () => {
        const sheet = stripComments(
            readFileSync(join(fontsDir, 'fonts.css'), 'utf8'),
        );
        const faces = new Map<string, Set<string>>();
        const add = (family: string, weight: string) =>
            faces.set(family, (faces.get(family) ?? new Set()).add(weight));

        for (const [, pkg, weight] of sheet.matchAll(
            /@import '(@fontsource\/[a-z-]+)\/(\d+)\.css';/g,
        )) {
            const css = readFileSync(
                require.resolve(`${pkg}/${weight}.css`),
                'utf8',
            );
            for (const family of declaredFamilies(css)) add(family, weight);
        }
        for (const [, face] of sheet.matchAll(/@font-face\s*{([^}]*)}/g)) {
            const [family] = declaredFamilies(face);
            const file = face.match(/url\('\.\/([^']+)'\)/)![1];
            expect(existsSync(join(fontsDir, file)), file).toBe(true);
            add(family, face.match(/font-weight:\s*(\d+)/)![1]);
        }

        for (const font of BUNDLED_FONTS) {
            expect([...(faces.get(font) ?? [])].sort(), font).toEqual([
                '400',
                '700',
            ]);
        }
    });
});
