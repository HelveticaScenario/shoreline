import ts from 'typescript';
import { describe, expect, test } from 'vitest';

import {
    bindingPosition,
    onLabelOutsideName,
    trackLabelView,
    withinPatchDiagnostics,
    withTrackLabelDiagnostics,
} from '../trackLabelBindings';

describe('trackLabelView', () => {
    test('appends one declaration per track and keeps the patch as written', () => {
        const source = `bass: $saw('c2')\nlead$2: $sine('c4')`;
        const view = trackLabelView(ts, source);
        expect(view.text.startsWith(source)).toBe(true);
        expect(view.text.slice(source.length)).toContain(
            "var bass = __trackBinding($saw('c2'))",
        );
        expect(view.text.trimEnd().endsWith('export {};')).toBe(true);
    });

    test('a patch with a parse error at its end still binds its tracks', () => {
        const source = `bass: $saw('c2')\nbass.`;
        const view = trackLabelView(ts, source);
        expect(view.bindings).toHaveLength(1);
        expect(view.endDiagnostics).toHaveLength(1);
        const file = ts.createSourceFile('a.js', view.text, 99, true);
        const diagnostics = withTrackLabelDiagnostics(
            ts,
            file,
            view,
            (file as unknown as { parseDiagnostics: ts.Diagnostic[] })
                .parseDiagnostics,
        );
        expect(diagnostics.map((d) => d.start)).toEqual([source.length]);
    });

    test('a patch that leaves a block open binds nothing', () => {
        const view = trackLabelView(ts, `bass: $saw('c2')\nif (1) {`);
        expect(view.bindings).toEqual([]);
        expect(view.text).toBe(`bass: $saw('c2')\nif (1) {`);
    });

    test('a request on a label name is asked of its declaration', () => {
        const source = `S_bass$2: $saw('c2')`;
        const view = trackLabelView(ts, source);
        const [binding] = view.bindings;
        expect(bindingPosition(view, 3)).toBe(binding.declStart + 1);
        expect(onLabelOutsideName(view, 0)).toBe(true);
        expect(onLabelOutsideName(view, 3)).toBe(false);
        expect(onLabelOutsideName(view, 7)).toBe(true);
    });

    test('a diagnostic on a declared name moves onto the label', () => {
        const source = `Math: $saw('c2')`;
        const view = trackLabelView(ts, source);
        const [binding] = view.bindings;
        const [moved] = withinPatchDiagnostics(view, [
            {
                category: ts.DiagnosticCategory.Error,
                code: 2403,
                file: undefined,
                length: 4,
                messageText: 'x',
                start: binding.declStart,
            },
            {
                category: ts.DiagnosticCategory.Error,
                code: 2339,
                file: undefined,
                length: 1,
                messageText: 'y',
                start: binding.declEnd + 5,
            },
        ]);
        expect(moved).toMatchObject({ length: 4, start: 0 });
        expect(moved.messageText).toMatch(/Math is already taken/);
    });
});
