/**
 * The worker keeps every patch buffer and docs example in one program. Each
 * is a module of its own, so no model's top-level names reach another, and
 * what the worker answers about a shared lib name keeps examples apart.
 */

import ts from 'typescript';
import { describe, expect, test } from 'vitest';
import schemas from '@modular/core/schemas.json';
import { buildLibSource } from '../../../../main/dsl/typescriptLibGen';
import { EDITOR_COMPILER_OPTIONS } from '../compilerOptions';
import {
    DOCS_EXAMPLE_ROOT,
    isDocsExample,
    isVisibleFrom,
} from '../docsExampleFiles';
import { trackLabelView } from '../trackLabelBindings';

const LIB_FILE = 'file:///modular/dsl-lib.d.ts';
const libSource = buildLibSource(schemas);

/** Each model's own text, before the track bindings the worker appends. */
const sources = new Map<string, string>();

/** A language service over `files` and the DSL lib, as the worker reads them. */
function serviceFor(files: Record<string, string>): ts.LanguageService {
    const texts = new Map<string, string>([
        [LIB_FILE, libSource],
        ...Object.entries(files).map(([name, text]): [string, string] => {
            sources.set(name, text);
            return [name, trackLabelView(ts, text).text];
        }),
    ]);
    const options: ts.CompilerOptions = {
        ...EDITOR_COMPILER_OPTIONS,
        lib: ['lib.esnext.d.ts'],
    };
    const host: ts.LanguageServiceHost = {
        fileExists: (name) => texts.has(name) || ts.sys.fileExists(name),
        getCompilationSettings: () => options,
        getCurrentDirectory: () => '/',
        getDefaultLibFileName: (o) => ts.getDefaultLibFilePath(o),
        getScriptFileNames: () => [...texts.keys()],
        getScriptSnapshot: (name) => {
            const text = texts.get(name) ?? ts.sys.readFile(name);
            return text === undefined
                ? undefined
                : ts.ScriptSnapshot.fromString(text);
        },
        getScriptVersion: () => '1',
        readFile: (name) => texts.get(name) ?? ts.sys.readFile(name),
    };
    return ts.createLanguageService(host);
}

/** Every diagnostic in `file`'s own text, as `code: message`. */
function errorsIn(service: ts.LanguageService, file: string): string[] {
    const length = sources.get(file)?.length ?? 0;
    return [
        ...service.getSyntacticDiagnostics(file),
        ...service.getSemanticDiagnostics(file),
    ]
        .filter((d) => (d.start ?? 0) < length)
        .map(
            (d) =>
                `${d.code}: ${ts.flattenDiagnosticMessageText(d.messageText, '\n')}`,
        );
}

const PATCH = 'file:///workspace/patch.js';
const OTHER_PATCH = 'file:///workspace/other.js';
const EXAMPLE = `${DOCS_EXAMPLE_ROOT}0.js`;
const OTHER_EXAMPLE = `${DOCS_EXAMPLE_ROOT}1.js`;

describe('model isolation', () => {
    test('two patches declare the same names without colliding', () => {
        const service = serviceFor({
            [OTHER_PATCH]: `const lead = $saw('c3')\nlead.scope()`,
            [PATCH]: `const lead = $sine('c4')\nlead.scope()`,
        });
        expect(errorsIn(service, PATCH)).toEqual([]);
        expect(errorsIn(service, OTHER_PATCH)).toEqual([]);
    });

    test("an example's names never reach a patch", () => {
        const patch = `const lead = $sine('c4')\nlead.scope()\nexampleOn`;
        const service = serviceFor({
            [EXAMPLE]: `const lead = $saw('c3')\nconst exampleOnly = 1\nlead.scope()`,
            [PATCH]: patch,
        });
        expect(
            errorsIn(service, PATCH).filter((e) => !e.includes('exampleOn')),
        ).toEqual([]);
        const names = service
            .getCompletionsAtPosition(PATCH, patch.length, {})
            ?.entries.map((e) => e.name);
        expect(names).toContain('lead');
        expect(names).not.toContain('exampleOnly');
    });

    test('track labels bind their names in each model alone', () => {
        const patch = `bass: $saw('c2')\nlead: $sine('c4').amp(bass.level)`;
        const service = serviceFor({
            [EXAMPLE]: `bass: $saw('c1')\nbass.scope()`,
            [OTHER_EXAMPLE]: `bass: $sine('c1')`,
            [PATCH]: patch,
        });
        expect(errorsIn(service, PATCH)).toEqual([]);
        expect(errorsIn(service, EXAMPLE)).toEqual([]);
        expect(errorsIn(service, OTHER_EXAMPLE)).toEqual([]);
        const use = patch.lastIndexOf('bass');
        const definitions = service.getDefinitionAtPosition(PATCH, use);
        expect(definitions?.map((d) => d.fileName)).toEqual([PATCH]);
    });

    test("references to a lib name stay in the asker's side", () => {
        const patch = `$sine('c4').scope()`;
        const example = `$sine('c3').scope()`;
        const service = serviceFor({
            [EXAMPLE]: example,
            [OTHER_EXAMPLE]: example,
            [OTHER_PATCH]: patch,
            [PATCH]: patch,
        });
        const filesSeenFrom = (from: string) =>
            new Set(
                service
                    .getReferencesAtPosition(from, 1)
                    ?.map((r) => r.fileName)
                    .filter((file) => isVisibleFrom(from, file)),
            );
        expect(filesSeenFrom(PATCH)).toEqual(
            new Set([PATCH, OTHER_PATCH, LIB_FILE]),
        );
        expect(filesSeenFrom(EXAMPLE)).toEqual(new Set([EXAMPLE, LIB_FILE]));
    });

    test('docs examples live apart from every buffer', () => {
        expect(isDocsExample(EXAMPLE)).toBe(true);
        expect(isDocsExample(PATCH)).toBe(false);
        expect(isDocsExample('file:///shoreline-docs.mjs')).toBe(false);
    });
});
