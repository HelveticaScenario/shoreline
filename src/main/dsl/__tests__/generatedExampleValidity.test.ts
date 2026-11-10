/**
 * Validates that every documented code example evaluates to a valid patch.
 *
 * Examples come from three sources:
 *   1. Each module's structured docs (`docs.examples` and section examples),
 *      declared in its Rust `ModuleDocs` const and carried by `schemas.json`.
 *   2. `@example` JSDoc tags hand-written in `typescriptLibGen.ts`.
 *   3. ```js fenced blocks in hand-written JSDoc there.
 *
 * Each one must run through `executePatchScript` without error and produce a
 * graph that passes the engine's own `validatePatchGraph`. A failure here
 * means an example a user could copy out of the editor does not work — fix
 * it at its source.
 */

import { describe, expect, test } from 'vitest';
import { validatePatchGraph } from '@modular/core';
import schemas from '@modular/core/schemas.json';
import { executePatchScript } from '../executor';
import {
    DOC_EXECUTION_OPTIONS,
    allDocExamples,
    isTypeLevelExample,
} from './docExamples';

// ─── The check ──────────────────────────────────────────────────────────────

const allExamples = allDocExamples();
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
