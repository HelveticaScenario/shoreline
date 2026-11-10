/**
 * Golden outputs of every static analysis of patch source, over the
 * documented examples and the fixtures beside this file. A
 * change to how patch code is parsed or walked must leave these files
 * untouched unless it means to change what an analyzer reports; such a change
 * updates them with `vitest run dslAstGolden -u` and the diff shows exactly
 * which outputs moved.
 *
 * The harness reads the live module schemas and generated DSL lib, so a
 * module or doc change also moves these files.
 *
 * `GOLDEN_EXTRA_DIR=<folder>` also records every `.js` / `.mjs` patch under
 * that folder, into `GOLDEN_EXTRA_OUT` (default `<folder>/.golden/`), for
 * checking against private patches that never enter the repo.
 */
import { createHash } from 'node:crypto';
import { existsSync, readdirSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { describe, expect, test } from 'vitest';

import {
    allDocExamples,
    isTypeLevelExample,
} from '../../main/dsl/__tests__/docExamples';
import { FIXTURES } from './fixtures';
import { goldenRecord } from './goldenRecords';

const GOLDEN_DIR = path.join(__dirname, '__golden__');

interface Entry {
    name: string;
    source: string;
    file: string;
}

function hashOf(source: string): string {
    return createHash('sha1').update(source).digest('hex').slice(0, 10);
}

function docEntries(): Entry[] {
    const byHash = new Map<string, Entry>();
    for (const example of allDocExamples()) {
        if (isTypeLevelExample(example.code)) {
            continue;
        }
        const name = `${example.kind}-${hashOf(example.code)}`;
        byHash.set(name, {
            file: path.join(GOLDEN_DIR, `${name}.json`),
            name,
            source: example.code,
        });
    }
    for (const [label, source] of Object.entries(FIXTURES)) {
        const name = `fixture-${label}`;
        byHash.set(name, {
            file: path.join(GOLDEN_DIR, `${name}.json`),
            name,
            source,
        });
    }
    return [...byHash.values()].sort((a, b) => (a.name < b.name ? -1 : 1));
}

function extraEntries(dir: string, outDir: string): Entry[] {
    const root = path.resolve(dir);
    const out: Entry[] = [];
    const walk = (folder: string) => {
        for (const dirent of readdirSync(folder, { withFileTypes: true })) {
            const full = path.join(folder, dirent.name);
            if (dirent.isDirectory()) {
                const skipped =
                    full === outDir ||
                    dirent.name.startsWith('.') ||
                    dirent.name === 'node_modules';
                if (!skipped) {
                    walk(full);
                }
            } else if (/\.m?js$/.test(dirent.name)) {
                const rel = path.relative(root, full);
                out.push({
                    file: path.join(outDir, `${rel}.json`),
                    name: rel,
                    source: readFileSync(full, 'utf8'),
                });
            }
        }
    };
    walk(root);
    return out.sort((a, b) => (a.name < b.name ? -1 : 1));
}

const entries = docEntries();

describe('static analysis golden outputs: doc examples', () => {
    test('the corpus is substantial', () => {
        expect(entries.length).toBeGreaterThan(80);
    });

    test.each(entries)('$name', async ({ file, source }) => {
        await expect(goldenRecord(source)).toMatchFileSnapshot(file);
    });

    test('no golden file outlives its example', () => {
        const expected = new Set(entries.map((e) => path.basename(e.file)));
        const stale = existsSync(GOLDEN_DIR)
            ? readdirSync(GOLDEN_DIR).filter((f) => !expected.has(f))
            : [];
        expect(
            stale,
            `delete these from ${GOLDEN_DIR}: ${stale.join(' ')}`,
        ).toEqual([]);
    });
});

const extraDir = process.env.GOLDEN_EXTRA_DIR;
const extraOut =
    process.env.GOLDEN_EXTRA_OUT ??
    (extraDir ? path.join(extraDir, '.golden') : '');

describe.runIf(extraDir)('static analysis golden outputs: extra dir', () => {
    // A large patch has thousands of quick-fix offsets to visit.
    test.each(extraDir ? extraEntries(extraDir, path.resolve(extraOut)) : [])(
        '$name',
        { timeout: 600_000 },
        async ({ file, source }) => {
            await expect(goldenRecord(source)).toMatchFileSnapshot(file);
        },
    );
});
