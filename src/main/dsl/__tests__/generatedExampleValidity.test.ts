/**
 * Validates that every code example embedded in the generated DSL type
 * definitions evaluates to a valid patch.
 *
 * The generated `dsl.d.ts` (produced by `buildLibSource`) is the documentation
 * surface developers read in Monaco. Its examples come from two sources:
 *   1. `@example` JSDoc tags hand-written in `typescriptLibGen.ts`.
 *   2. ```js fenced blocks inside each module's Rust doc comment, carried
 *      through `schemas.json` into the generated JSDoc.
 *
 * This test regenerates that surface, extracts every example, and asserts each
 * one runs through `executePatchScript` without error and produces a graph that
 * passes the engine's own `validatePatchGraph`. A failure here means a code
 * example a user could copy out of the editor does not actually work — fix it
 * at its source (the `@example` tag or the Rust doc comment).
 */

import { describe, expect, test } from 'vitest';
import { validatePatchGraph } from '@modular/core';
import schemas from '@modular/core/schemas.json';
import { buildLibSource } from '../typescriptLibGen';
import { executePatchScript } from '../executor';
import {
    DOC_EXECUTION_OPTIONS,
    extractDocExamples,
    isTypeLevelExample,
} from './docExamples';

// ─── The check ──────────────────────────────────────────────────────────────

const generated = buildLibSource(schemas);
const allExamples = extractDocExamples(generated);
const patchExamples = allExamples.filter((e) => !isTypeLevelExample(e.code));

describe('generated DSL example extraction', () => {
    test('finds a substantial number of examples', () => {
        // Guards against the extractor silently matching nothing.
        expect(allExamples.length).toBeGreaterThan(80);
        expect(patchExamples.length).toBeGreaterThan(80);
    });
});

describe('every generated code example evaluates to a valid patch', () => {
    test.each(patchExamples)('[$kind #$ordinal] $label', ({ code }) => {
        let result: ReturnType<typeof executePatchScript>;
        try {
            result = executePatchScript(code, schemas, DOC_EXECUTION_OPTIONS);
        } catch (error) {
            throw new Error(
                `Example failed to execute:\n\n${code}\n\n→ ${
                    error instanceof Error ? error.message : String(error)
                }`,
                { cause: error },
            );
        }

        const errors = validatePatchGraph(result.patch);
        if (errors.length > 0) {
            const detail = errors
                .map((e) => `  • ${e.field}: ${e.message}`)
                .join('\n');
            throw new Error(
                `Example built an invalid patch:\n\n${code}\n\n${detail}`,
            );
        }
    });
});
