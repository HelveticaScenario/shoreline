/** Shared fixtures for the control quick-fix tests. */
import { expect } from 'vitest';

import schemas from '@modular/core/schemas.json';

import { buildLibSource } from '../../../main/dsl/typescriptLibGen';
import { indexApiSignatures } from '../apiSignatures';
import { computeControlQuickFixes } from '../controlQuickFix';
import type { ControlApi, ControlQuickFix } from '../controlQuickFix';

export const API: ControlApi = {
    index: indexApiSignatures(buildLibSource(schemas)),
    schemas,
};

/** Cursor marked with '|'; returns the source without the marker + offset. */
function fixture(text: string): { source: string; offset: number } {
    const offset = text.indexOf('|');
    expect(offset).toBeGreaterThanOrEqual(0);
    return { offset, source: text.slice(0, offset) + text.slice(offset + 1) };
}

export function fixesAt(
    text: string,
    quote = "'",
): { source: string; fixes: ControlQuickFix[] } {
    const { source, offset } = fixture(text);
    return {
        fixes: computeControlQuickFixes(source, offset, API, quote),
        source,
    };
}

export function addFix(
    fixes: ControlQuickFix[],
    param: string,
): ControlQuickFix {
    const fix = fixes.find((f) => f.title === `Add slider for '${param}'`);
    expect(fix).toBeDefined();
    return fix!;
}

export function apply(source: string, fix: ControlQuickFix): string {
    return (
        source.slice(0, fix.span.start) +
        fix.newText +
        source.slice(fix.span.end)
    );
}

/** The `$slider(...)` call that ends at the caret after applying `fix`. */
export function sliderBeforeCaret(
    source: string,
    fix: ControlQuickFix,
): string {
    const out = apply(source, fix);
    const at = fix.span.start + fix.caretOffset;
    return out.slice(out.lastIndexOf('$slider(', at), at);
}

export function wrapFix(fixes: ControlQuickFix[]): ControlQuickFix {
    const wraps = fixes.filter((f) => f.kind === 'wrap');
    expect(wraps).toHaveLength(1);
    return wraps[0];
}

export function addTitles(fixes: ControlQuickFix[]): string[] {
    return fixes.filter((f) => f.kind === 'add').map((f) => f.title);
}
