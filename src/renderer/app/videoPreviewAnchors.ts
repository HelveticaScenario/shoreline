import type { editor, IRange } from 'monaco-editor';
import { FIRST_LINE_COLUMN_OFFSET } from '../../shared/dsl/spanTypes';
import type { VideoPreviewSite } from '../../shared/video/videoGraph';
import type { VideoPreviewZone } from '../types/editor';
import { type ModelRanges, createModelRanges } from './modelAnchors';
import { resolveScopeCallRange } from './scopeCallRange';

export interface VideoPreviewAnchors {
    zones: VideoPreviewZone[];
    /** Tracked ranges, one per resolvable zone; null when none resolved. */
    decorations: ModelRanges | null;
}

/**
 * Anchors each `$v.preview` call with a tracked range so its panel follows the
 * call as the document is edited. A call whose line no longer exists (the
 * source changed during the async submit) gets a zone with no anchor, which
 * the editor hides.
 */
export function createVideoPreviewAnchors(
    editorInstance: editor.IStandaloneCodeEditor,
    sites: VideoPreviewSite[],
    callSiteSpans:
        | Record<string, { startLine: number; endLine: number }>
        | undefined,
    file: string,
): VideoPreviewAnchors {
    const model = editorInstance.getModel();
    const ranges: IRange[] = [];
    const zones = sites.map((site): VideoPreviewZone => {
        const loc = site.sourceLocation;
        const range =
            model && loc
                ? resolveScopeCallRange(
                      model,
                      {
                          column:
                              loc.line === 1
                                  ? loc.column - FIRST_LINE_COLUMN_OFFSET
                                  : loc.column,
                          line: loc.line,
                      },
                      callSiteSpans?.[`${loc.line}:${loc.column}`],
                  )
                : null;
        if (range) {
            ranges.push(range);
        }
        return {
            decorationIndex: range ? ranges.length - 1 : null,
            file,
            index: site.index,
            view: site.view,
        };
    });
    return {
        decorations:
            model && ranges.length > 0
                ? createModelRanges(model, ranges)
                : null,
        zones,
    };
}
