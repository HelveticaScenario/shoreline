/**
 * The kind of file a string literal names, told from its call, over the
 * tables main builds from the schemas.
 */

import { describe, expect, test } from 'vitest';
import ts from 'typescript';
import schemas from '@modular/core/schemas.json';
import { assetKindAt } from '../../../shared/dsl/assetArguments';
import { buildSignalSiteTables } from '../signalSiteTables';
import { buildLibSource } from '../typescriptLibGen';

const tables = buildSignalSiteTables(schemas);
const chainable = new Set(
    Object.entries(tables.modules)
        .filter(([, info]) => info.chainable)
        .map(([name]) => name),
);

/** The kind at the `|` in `source`, which marks the cursor. */
function kindAt(source: string) {
    const position = source.indexOf('|');
    const text = source.replace('|', '');
    const sourceFile = ts.createSourceFile(
        'patch.js',
        text,
        ts.ScriptTarget.ES2020,
        true,
    );
    return assetKindAt(ts, sourceFile, tables.assets, chainable, position);
}

describe('asset kind of a string literal', () => {
    test("a module's file param", () => {
        expect(kindAt("$sampler('/wa|', 5)")).toBe('wav');
        expect(kindAt("$wavetable('c3', '/|')")).toBe('wav');
        expect(kindAt("$grains('c3', 'builtin/|', 5)")).toBe('wav');
        expect(kindAt("$mi.sixOp('c3', '/dx7/|')")).toBe('syx');
    });

    test('the chain form of a module counts its receiver', () => {
        expect(kindAt("$saw('c3').wavetable('/|')")).toBe('wav');
        expect(kindAt("const p = $saw('c3')\np.wavetable('/|')")).toBe('wav');
    });

    test('the lib declares the calls that take a file without being modules', () => {
        const lib = buildLibSource(schemas);
        expect(lib).toMatch(/function \$wavMeta\(path: string/);
        expect(lib).toMatch(/declare const \$v: \{[^]*?\bimage\(path: string/);
        expect(lib).toMatch(
            /declare const \$v: \{[^]*?\bvideo\(\s*path: string/,
        );
    });

    test('calls that take a file without being modules', () => {
        expect(kindAt("$wavMeta('/|')")).toBe('wav');
        expect(kindAt("$v.image('/|')")).toBe('image');
        expect(kindAt("$v.video('/|')")).toBe('video');
    });

    test('a literal its call does not tell offers every kind', () => {
        expect(kindAt("$sampler($saw('c3'), '/|')")).toBeUndefined();
        expect(kindAt("const path = '/|'")).toBeUndefined();
        expect(kindAt("$sampler(['/|'], 5)")).toBeUndefined();
        expect(kindAt("$saw('/|')")).toBeUndefined();
        expect(kindAt("f('/|')")).toBeUndefined();
    });

    test('an unclosed literal still tells', () => {
        expect(kindAt("$sampler('/wa|")).toBe('wav');
    });

    test('outside the literal nothing tells', () => {
        expect(kindAt("$sampler(|'/wa', 5)")).toBeUndefined();
    });
});
