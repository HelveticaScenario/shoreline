import type { IRange, editor } from 'monaco-editor';
import ts from 'typescript';
import { vi } from 'vitest';

import { parseDsl } from '../../../shared/dsl/ast/parse';
import { parseTrackLabel } from '../../../shared/dsl/trackLabels';
import type { VuMeterDef } from '../../../shared/dsl/vuMeterTypes';
import type { ModelAnchors } from '../../app/modelAnchors';
import { MixerController } from '../MixerController';

/** The buffer the harness's tracks are built from. */
export const SOURCE_ID = 'buffer-1';

/** A track's meter with every control editable; `over` replaces fields. */
export function track(key: string, over: Partial<VuMeterDef> = {}): VuMeterDef {
    return {
        baseChannel: 0,
        channels: 2,
        gain: { locked: false, moduleId: `gain-${key}`, value: 5 },
        key,
        label: null,
        moduleId: `tap-${key}`,
        mute: false,
        muteModuleId: `mute-${key}`,
        pan: { locked: false, moduleId: `pan-${key}`, value: 0 },
        portName: 'output',
        solo: false,
        sourceLocation: { column: 1, line: 1 },
        ...over,
    };
}

/** The offset of the label in `source` naming track `name`, or null. */
function labelOffset(source: string, name: string): number | null {
    const file = parseDsl(source);
    const label = file.statements
        .filter(ts.isLabeledStatement)
        .map((st) => st.label)
        .find((l) => {
            const parsed = parseTrackLabel(l.text);
            return !('error' in parsed) && parsed.name === name;
        });
    return label ? label.getStart(file) : null;
}

/**
 * A mixer showing `outputs`, built from `source` in an editor that shows
 * that buffer. `calls` records the engine's `setModuleParam` calls and
 * `text()` reads the editor's document.
 */
export function mixerOn(source: string, outputs: VuMeterDef[]) {
    let text = source;
    const positionAt = (offset: number) => {
        const before = text.slice(0, offset).split('\n');
        return { column: before.at(-1)!.length + 1, lineNumber: before.length };
    };
    const offsetAt = (lineNumber: number, column: number) =>
        text
            .split('\n')
            .slice(0, lineNumber - 1)
            .reduce((sum, line) => sum + line.length + 1, 0) +
        column -
        1;
    const model = {
        getPositionAt: positionAt,
        getValue: () => text,
        pushEditOperations: (
            _selections: unknown,
            edits: { range: IRange; text: string }[],
        ) => {
            for (const { range, text: inserted } of edits) {
                const start = offsetAt(
                    range.startLineNumber,
                    range.startColumn,
                );
                const end = offsetAt(range.endLineNumber, range.endColumn);
                text = text.slice(0, start) + inserted + text.slice(end);
            }
        },
    };
    const anchors: ModelAnchors = {
        clear: () => {},
        offsetOf: (index) => labelOffset(text, outputs[index].key),
    };
    const setModuleParam = vi.fn(
        (_moduleId: string, _moduleType: string, _params: object) =>
            Promise.resolve(),
    );
    const mixer = new MixerController({ setModuleParam });
    mixer.edits.attachEditor({
        getModel: () => model,
        getSelections: () => null,
    } as unknown as editor.IStandaloneCodeEditor);
    mixer.edits.setVisibleBuffer(SOURCE_ID);
    mixer.commit(outputs, anchors, SOURCE_ID);
    return { calls: setModuleParam.mock.calls, mixer, text: () => text };
}
