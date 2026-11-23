import type { editor, IRange } from 'monaco-editor';
import type { DSLExecuteResult } from '../../shared/ipcTypes';
import type { ButtonDefinition } from '../../shared/dsl/buttonTypes';
import type { SliderDefinition } from '../../shared/dsl/sliderTypes';
import {
    FIRST_LINE_COLUMN_OFFSET,
    setActiveInterpolationResolutions,
} from '../../shared/dsl/spanTypes';
import type { VuMeterDef } from '../../shared/dsl/vuMeterTypes';
import type { EqPanelController } from '../eq/eqPanelController';
import { type EqPanelSetup, disposeEqPanelSetup } from '../eq/eqPanelSetup';
import type { ScopeView } from '../types/editor';
import { type ControlAnchors, createControlAnchors } from './controlAnchors';
import {
    type ModelAnchors,
    type ModelRanges,
    createModelAnchors,
    createModelRanges,
} from './modelAnchors';
import { scopeBufferKeyFromLane } from './oscilloscope';
import { resolveScopeCallRange } from './scopeCallRange';
import {
    type VideoPreviewAnchors,
    createVideoPreviewAnchors,
} from './videoPreviewAnchors';

type CodeEditor = editor.IStandaloneCodeEditor;
type CallSiteSpans = DSLExecuteResult['callSiteSpans'];

/** What the editor and control panel show of the running patch. */
export interface RunningPatchUi {
    scopeViews: ScopeView[];
    /** Tracked ranges spanning each scope() call, which Monaco moves as the
     *  document is edited; `ScopeView.decorationIndex` indexes them. */
    scopeDecorations: ModelRanges | null;
    sliderDefs: SliderDefinition[];
    buttonDefs: ButtonDefinition[];
    /** Anchors on the patch's control calls. */
    controlAnchors: ControlAnchors | null;
    /** The patch's `$v.preview` panels. */
    videoPreviews: VideoPreviewAnchors;
    /** Stable id of the buffer the patch was evaluated from. */
    sourceId: string | null;
}

/** Everything one patch run puts on screen, built when the patch is
 *  submitted and shown once the audio thread applies it. */
export interface PatchUi extends RunningPatchUi {
    vuOutputs: VuMeterDef[];
    /** One anchor per meter on its label's first character, index-aligned
     *  with `vuOutputs`; the main meter has none. */
    vuAnchors: ModelAnchors | null;
    eqPanels: EqPanelSetup;
    interpolationResolutions: ReturnType<typeof interpolationMap>;
}

export const NO_PATCH_UI: RunningPatchUi = {
    buttonDefs: [],
    controlAnchors: null,
    scopeDecorations: null,
    scopeViews: [],
    sliderDefs: [],
    sourceId: null,
    videoPreviews: { decorations: null, zones: [] },
};

function interpolationMap(result: DSLExecuteResult) {
    return result.interpolationResolutions
        ? new Map(Object.entries(result.interpolationResolutions))
        : undefined;
}

/** Highlight template-literal interpolations from `result`'s analysis,
 *  which succeeds even when the patch fails to apply. */
export function applyInterpolationResolutions(result: DSLExecuteResult): void {
    const map = interpolationMap(result);
    if (map) {
        setActiveInterpolationResolutions(map);
    }
}

function buildScopes(
    result: DSLExecuteResult,
    editorInstance: CodeEditor | null,
    file: string,
    callSiteSpans: CallSiteSpans,
): Pick<RunningPatchUi, 'scopeViews' | 'scopeDecorations'> {
    const model = editorInstance?.getModel();
    const scopeViews: ScopeView[] = [];
    const ranges: IRange[] = [];
    (result.appliedPatch?.scopes ?? []).forEach((scope, i) => {
        // Panel feeds (the EQ spectrum) have no view of their own.
        if ((scope as { headless?: boolean }).headless) {
            return;
        }
        const laneKeys = scope.lanes.map((lane: any) =>
            scopeBufferKeyFromLane(
                lane,
                scope.msPerFrame,
                scope.triggerThreshold,
            ),
        );
        const loc = (scope as any).sourceLocation as
            | { line: number; column: number }
            | undefined;
        // A scope whose call site cannot be resolved (source edited during
        // the async round-trip) gets no decoration and a null
        // decorationIndex: its zone is hidden.
        const range =
            model && loc
                ? resolveScopeCallRange(
                      model,
                      loc,
                      callSiteSpans?.[`${loc.line}:${loc.column}`],
                  )
                : null;
        scopeViews.push({
            decorationIndex: range ? ranges.length : null,
            file,
            key:
                laneKeys.length > 0
                    ? `scope:${i}:${laneKeys.join('+')}`
                    : `scope:${i}:empty`,
            laneKeys,
            range: scope.range ?? [-5, 5],
            split: scope.split ?? null,
        });
        if (range) {
            ranges.push(range);
        }
    });
    return {
        scopeDecorations:
            model && ranges.length > 0
                ? createModelRanges(model, ranges)
                : null,
        scopeViews,
    };
}

function buildVuAnchors(
    model: editor.ITextModel | null | undefined,
    vuOutputs: VuMeterDef[],
): ModelAnchors | null {
    if (!model || vuOutputs.length === 0) {
        return null;
    }
    return createModelAnchors(
        model,
        vuOutputs.map((vu) => {
            const loc = vu.sourceLocation;
            // Captured columns are V8 columns, shifted by the executor's
            // indent on line 1 only.
            return loc
                ? {
                      column:
                          loc.column -
                          (loc.line === 1 ? FIRST_LINE_COLUMN_OFFSET : 0),
                      line: Math.min(loc.line, model.getLineCount()),
                  }
                : null;
        }),
    );
}

/**
 * Build the UI of a successful patch run against the editor's document:
 * `file` is the path identity and `sourceId` the stable id of the buffer it
 * was evaluated from.
 */
export function buildPatchUi(
    result: DSLExecuteResult,
    editorInstance: CodeEditor | null,
    eqController: EqPanelController,
    file: string,
    sourceId: string | null,
): PatchUi {
    const model = editorInstance?.getModel();
    const { callSiteSpans } = result;
    const vuOutputs = (result.appliedPatch?.vuMeters ?? []) as VuMeterDef[];
    const sliderDefs = result.sliders ?? [];
    const buttonDefs = result.buttons ?? [];
    return {
        ...buildScopes(result, editorInstance, file, callSiteSpans),
        buttonDefs,
        controlAnchors: model
            ? createControlAnchors(model, [
                  ...sliderDefs.map((s) => ({
                      fnName: '$slider',
                      moduleId: s.moduleId,
                      sourceLocation: s.sourceLocation,
                  })),
                  ...buttonDefs.map((b) => ({
                      fnName: b.mode === 'gate' ? '$btn' : '$toggleBtn',
                      moduleId: b.moduleId,
                      sourceLocation: b.sourceLocation,
                  })),
              ])
            : null,
        eqPanels: eqController.stage(result, editorInstance),
        interpolationResolutions: interpolationMap(result),
        sliderDefs,
        sourceId,
        videoPreviews: editorInstance
            ? createVideoPreviewAnchors(
                  editorInstance,
                  result.videoPreviews ?? [],
                  callSiteSpans,
                  file,
              )
            : { decorations: null, zones: [] },
        vuAnchors: buildVuAnchors(model, vuOutputs),
        vuOutputs,
    };
}

/** Drop the editor decorations of a patch UI that is leaving the screen. */
export function releaseRunningPatchUi(ui: RunningPatchUi): void {
    ui.scopeDecorations?.clear();
    ui.controlAnchors?.dispose();
    ui.videoPreviews.decorations?.clear();
}

/** Drop everything a patch UI that never reached the screen holds. */
export function disposePatchUi(ui: PatchUi | null): void {
    if (!ui) {
        return;
    }
    releaseRunningPatchUi(ui);
    ui.vuAnchors?.clear();
    disposeEqPanelSetup(ui.eqPanels);
}
