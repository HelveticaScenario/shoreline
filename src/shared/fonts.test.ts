import { readFileSync } from 'node:fs';
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
    it('imports the font sheet before any rule, where CSS honours @import', () => {
        const css = stripComments(
            readFileSync(join(root, 'src/renderer/index.css'), 'utf8'),
        ).trimStart();
        expect(css.startsWith("@import './assets/fonts/fonts.css';")).toBe(
            true,
        );
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
