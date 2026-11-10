/**
 * Every documented code example: each module's structured docs
 * (`docs.examples` and section examples, declared in its Rust `ModuleDocs`
 * const and carried by `schemas.json`), plus the `@example` tags and ```js
 * fences in the hand-written JSDoc of the generated DSL lib. Shared by the
 * tests that run every documented example.
 */
import schemas from '@modular/core/schemas.json';

import type { DSLExecutionOptions, WavsFolderNode } from '../executor';
import { buildLibSource } from '../typescriptLibGen';

export interface DocExample {
    /** Where the example came from: a module's docs, an `@example` tag, or
     *  a ```js fence in hand-written JSDoc. */
    kind: 'module' | 'tag' | 'fence';
    /** Ordinal within the generated source, for stable test labels. */
    ordinal: number;
    /** The example source code. */
    code: string;
    /** A short, human-readable label for the test name. */
    label: string;
}

/** Strip the leading ` * ` JSDoc gutter from every line of a comment block. */
function stripJsDocGutter(block: string): string {
    return block
        .split('\n')
        .map((line) => line.replace(/^[ \t]*\*?[ \t]?/, ''))
        .join('\n');
}

const FENCE_RE = /```(?:js|javascript|ts|typescript)?[ \t]*\r?\n([\s\S]*?)```/g;

/** Collapse an example into a one-line label (first non-comment line). */
function makeLabel(code: string): string {
    const firstCode =
        code
            .split('\n')
            .map((l) => l.trim())
            .find((l) => l.length > 0 && !l.startsWith('//')) ?? code.trim();
    return firstCode.length > 72 ? `${firstCode.slice(0, 69)}...` : firstCode;
}

/**
 * Extract every code example from a generated `dsl.d.ts` string: `@example`
 * tags and ```js fenced blocks living inside JSDoc comments.
 */
export function extractDocExamples(generated: string): DocExample[] {
    const examples: DocExample[] = [];
    let ordinal = 0;

    const jsDocBlocks = generated.matchAll(/\/\*\*([\s\S]*?)\*\//g);
    for (const blockMatch of jsDocBlocks) {
        const clean = stripJsDocGutter(blockMatch[1]);

        // Fenced code blocks (module documentation carried from Rust).
        let withoutFences = clean;
        for (const fence of clean.matchAll(FENCE_RE)) {
            const code = fence[1].replace(/\s+$/, '');
            if (code.trim().length > 0) {
                examples.push({
                    kind: 'fence',
                    ordinal: ordinal++,
                    code,
                    label: makeLabel(code),
                });
            }
        }
        // Remove fences so `@example` parsing below can't see code inside them.
        withoutFences = clean.replace(FENCE_RE, '');

        // `@example` tags. An example body is the text on the tag line plus any
        // following lines up to the next block tag (`@param`, `@see`, …) or EOF.
        const lines = withoutFences.split('\n');
        for (let i = 0; i < lines.length; i++) {
            // Tolerate leftover indentation after the gutter strip so an
            // extra-indented `@example` is never silently dropped.
            const tag = lines[i].match(/^[ \t]*@example[ \t]?(.*)$/);
            if (!tag) continue;
            const body: string[] = [];
            if (tag[1].trim().length > 0) body.push(tag[1]);
            for (let j = i + 1; j < lines.length; j++) {
                if (/^[ \t]*@\w+/.test(lines[j])) break;
                body.push(lines[j]);
                i = j;
            }
            const code = body.join('\n').replace(/\s+$/, '');
            if (code.trim().length > 0) {
                examples.push({
                    kind: 'tag',
                    ordinal: ordinal++,
                    code,
                    label: makeLabel(code),
                });
            }
        }
    }

    return examples;
}

/** Every example in the modules' structured docs, sections included. */
function moduleExamples(): DocExample[] {
    return schemas.flatMap((module) =>
        [
            ...module.docs.examples,
            ...module.docs.sections.flatMap((s) => s.examples),
        ].map((e, ordinal) => ({
            code: e.code,
            kind: 'module' as const,
            label: `${module.name}: ${e.title}`,
            ordinal,
        })),
    );
}

/** Every documented example, each counted once: the generated JSDoc repeats
 *  each module's first example. */
export function allDocExamples(): DocExample[] {
    const fromModules = moduleExamples();
    const moduleCode = new Set(fromModules.map((e) => e.code));
    return [
        ...fromModules,
        ...extractDocExamples(buildLibSource(schemas)).filter(
            (e) => !moduleCode.has(e.code.replace(/^\/\/.*\n/, '')),
        ),
    ];
}

/**
 * Some examples document TypeScript *types*, not patches (e.g.
 * `type T = ElementsOf<[number[], string[]]>`). They can never be a patch, so
 * they are excluded from patch validation.
 */
export function isTypeLevelExample(code: string): boolean {
    return /^\s*(export\s+)?(type|interface|declare)\b/.test(code);
}

// A stub wavs folder so examples that read samples via `$wavs()` can build a
// graph without real audio files on disk. Covers every wav path referenced by
// the examples (`pad`, `strings`, `kick`, `tables/pad`).

const STUB_WAVS_TREE: WavsFolderNode = {
    pad: 'file',
    strings: 'file',
    kick: 'file',
    tables: { pad: 'file' },
};

const stubLoadWav: NonNullable<DSLExecutionOptions['loadWav']> = (path) => ({
    channels: 2,
    frameCount: 96_000,
    path,
    sampleRate: 48_000,
    duration: 2,
    bitDepth: 16,
    loops: [],
    cuePoints: [],
    mtime: 0,
});

const stubLoadMediaAudio: NonNullable<DSLExecutionOptions['loadMediaAudio']> = (
    path,
) => ({ ...stubLoadWav(`media:${path}`), bitDepth: 32 });

export const DOC_EXECUTION_OPTIONS: DSLExecutionOptions = {
    loadMediaAudio: stubLoadMediaAudio,
    sampleRate: 48_000,
    workspaceRoot: '/workspace',
    wavsFolderTree: STUB_WAVS_TREE,
    loadWav: stubLoadWav,
};
