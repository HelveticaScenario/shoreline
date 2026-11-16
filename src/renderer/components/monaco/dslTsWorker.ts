/**
 * Monaco's TypeScript worker, extended with the DSL's signal-site collector
 * so output-resolution hints reuse the program the editor already keeps warm,
 * with the extract and inline refactors Monaco's code actions leave out,
 * with a plain diagnostic for `$'…'` in place of the parser's, and with the
 * names track labels bind (see trackLabelBindings).
 */

import type {
    CodeFixAction,
    DefinitionInfo,
    Diagnostic,
    DocumentHighlights,
    FormatCodeSettings,
    InlayHint,
    NavigationTree,
    QuickInfo,
    ReferenceEntry,
    RenameInfo,
    RenameInfoOptions,
    RenameLocation,
    TextChange,
    TextSpan,
} from 'typescript';
import { initialize } from 'monaco-editor/esm/vs/common/initialize.js';
import { typescript as ts } from 'monaco-editor/esm/vs/language/typescript/lib/typescriptServices.js';
import { TypeScriptWorker } from 'monaco-editor/esm/vs/language/typescript/tsWorker.js';
import {
    collectSignalSites,
    type SignalSite,
    type SignalSiteTables,
} from '../../../shared/dsl/signalSites';
import {
    localRefactorEdits,
    localRefactors,
    type LocalRefactor,
    type LocalRefactorEdits,
    type RefactorStyle,
} from './localRefactors';
import {
    withDollarQuoteDiagnostics,
    withoutDollarQuoteTypeErrors,
} from './dollarQuotes';
import {
    bindingPosition,
    groupSuffixDiagnostics,
    mapSpan,
    onLabelOutsideName,
    trackLabelView,
    withinPatch,
    withinPatchDiagnostics,
    withTrackLabelDiagnostics,
    type TrackLabelView,
} from './trackLabelBindings';

class DslTypeScriptWorker extends TypeScriptWorker {
    private readonly views = new Map<
        string,
        { version: number; view: TrackLabelView }
    >();

    /** The track label view of a patch model, or undefined for a lib. */
    private view(fileName: string): TrackLabelView | undefined {
        const model = this._getModel(fileName);
        if (!model) {
            return undefined;
        }
        const cached = this.views.get(fileName);
        if (cached?.version === model.version) {
            return cached.view;
        }
        const view = trackLabelView(ts, model.getValue());
        this.views.set(fileName, { version: model.version, view });
        return view;
    }

    protected override _getScriptText(fileName: string): string | undefined {
        return this.view(fileName)?.text ?? super._getScriptText(fileName);
    }

    override async getSyntacticDiagnostics(
        fileName: string,
    ): Promise<Diagnostic[]> {
        const sourceFile = this.getLanguageService()
            .getProgram()
            ?.getSourceFile(fileName);
        const diagnostics = await super.getSyntacticDiagnostics(fileName);
        const view = this.view(fileName);
        if (!sourceFile || !view) {
            return diagnostics;
        }
        return TypeScriptWorker.clearFiles(
            withTrackLabelDiagnostics(
                ts,
                sourceFile,
                view,
                withDollarQuoteDiagnostics(ts, sourceFile, diagnostics),
            ),
        );
    }

    override async getSemanticDiagnostics(
        fileName: string,
    ): Promise<Diagnostic[]> {
        const sourceFile = this.getLanguageService()
            .getProgram()
            ?.getSourceFile(fileName);
        const diagnostics = await super.getSemanticDiagnostics(fileName);
        const view = this.view(fileName);
        if (!sourceFile || !view) {
            return diagnostics;
        }
        const checker = this.getLanguageService()
            .getProgram()!
            .getTypeChecker();
        return TypeScriptWorker.clearFiles(
            [
                ...withinPatchDiagnostics(
                    view,
                    withoutDollarQuoteTypeErrors(ts, sourceFile, diagnostics),
                ),
                ...groupSuffixDiagnostics(ts, checker, sourceFile, view),
            ].sort((a, b) => (a.start ?? 0) - (b.start ?? 0)),
        );
    }

    override async getSuggestionDiagnostics(
        fileName: string,
    ): Promise<Diagnostic[]> {
        const diagnostics = await super.getSuggestionDiagnostics(fileName);
        const view = this.view(fileName);
        return view ? withinPatch(view, diagnostics) : diagnostics;
    }

    /** `position` moved from a track label's name onto its declaration. */
    private at(fileName: string, position: number): number {
        const view = this.view(fileName);
        return view ? bindingPosition(view, position) : position;
    }

    /** Whether `span` in `fileName` lies within the patch's own text. */
    private inPatch(fileName: string, span: TextSpan): boolean {
        const view = this.view(fileName);
        return !view || span.start + span.length <= view.length;
    }

    /** `span` in `fileName` moved onto the patch's own text, or null. */
    private patchSpan(fileName: string, span: TextSpan): TextSpan | null {
        const view = this.view(fileName);
        return view ? mapSpan(view, span) : span;
    }

    /** `entries` with their spans moved onto the patch's own text. */
    private mapEntries<T extends { fileName: string; textSpan: TextSpan }>(
        entries: readonly T[] | undefined,
    ): T[] | undefined {
        return entries?.flatMap((entry) => {
            const textSpan = this.patchSpan(entry.fileName, entry.textSpan);
            if (!textSpan) {
                return [];
            }
            // Context and prefix spans describe the appended text; a moved
            // entry keeps only its own span.
            return textSpan === entry.textSpan
                ? [entry]
                : [{ ...entry, contextSpan: undefined, textSpan }];
        });
    }

    override async getDefinitionAtPosition(
        fileName: string,
        position: number,
    ): Promise<readonly DefinitionInfo[] | undefined> {
        return this.mapEntries(
            await super.getDefinitionAtPosition(
                fileName,
                this.at(fileName, position),
            ),
        );
    }

    override async getReferencesAtPosition(
        fileName: string,
        position: number,
    ): Promise<ReferenceEntry[] | undefined> {
        return this.mapEntries(
            await super.getReferencesAtPosition(
                fileName,
                this.at(fileName, position),
            ),
        );
    }

    override async findRenameLocations(
        fileName: string,
        position: number,
        findInStrings: boolean,
        findInComments: boolean,
        providePrefixAndSuffixTextForRename: boolean,
    ): Promise<readonly RenameLocation[] | undefined> {
        return this.mapEntries(
            await super.findRenameLocations(
                fileName,
                this.at(fileName, position),
                findInStrings,
                findInComments,
                providePrefixAndSuffixTextForRename,
            ),
        );
    }

    override async getDocumentHighlights(
        fileName: string,
        position: number,
        filesToSearch: string[],
    ): Promise<DocumentHighlights[] | undefined> {
        const highlights = await super.getDocumentHighlights(
            fileName,
            this.at(fileName, position),
            filesToSearch,
        );
        return highlights?.map((h) => ({
            ...h,
            highlightSpans: h.highlightSpans.flatMap((span) => {
                const textSpan = this.patchSpan(h.fileName, span.textSpan);
                return textSpan
                    ? [{ ...span, contextSpan: undefined, textSpan }]
                    : [];
            }),
        }));
    }

    override async getQuickInfoAtPosition(
        fileName: string,
        position: number,
    ): Promise<QuickInfo | undefined> {
        const info = await super.getQuickInfoAtPosition(
            fileName,
            this.at(fileName, position),
        );
        const textSpan = info && this.patchSpan(fileName, info.textSpan);
        return info && textSpan ? { ...info, textSpan } : undefined;
    }

    override async getRenameInfo(
        fileName: string,
        position: number,
        options: RenameInfoOptions,
    ): Promise<RenameInfo> {
        const view = this.view(fileName);
        if (view && onLabelOutsideName(view, position)) {
            return {
                canRename: false,
                localizedErrorMessage:
                    "Rename a track from its name, not its label's prefix or channel",
            };
        }
        const info = await super.getRenameInfo(
            fileName,
            this.at(fileName, position),
            options,
        );
        if (!info.canRename) {
            return info;
        }
        const triggerSpan = this.patchSpan(fileName, info.triggerSpan);
        return triggerSpan
            ? { ...info, triggerSpan }
            : { canRename: false, localizedErrorMessage: 'Nothing to rename' };
    }

    override async getNavigationTree(
        fileName: string,
    ): Promise<NavigationTree | undefined> {
        const tree = await super.getNavigationTree(fileName);
        const keep = (node: NavigationTree): NavigationTree | null => {
            const spans = node.spans.filter((span) =>
                this.inPatch(fileName, span),
            );
            if (spans.length === 0) {
                return null;
            }
            return {
                ...node,
                childItems: node.childItems
                    ?.map(keep)
                    .filter((n): n is NavigationTree => n !== null),
                spans,
            };
        };
        return tree && (keep(tree) ?? undefined);
    }

    override async getCodeFixesAtPosition(
        fileName: string,
        start: number,
        end: number,
        errorCodes: number[],
        formatOptions: FormatCodeSettings,
    ): Promise<readonly CodeFixAction[]> {
        const fixes = await super.getCodeFixesAtPosition(
            fileName,
            start,
            end,
            errorCodes,
            formatOptions,
        );
        return fixes.filter((fix) =>
            fix.changes.every((change) =>
                change.textChanges.every((c) =>
                    this.inPatch(change.fileName, c.span),
                ),
            ),
        );
    }

    override async getFormattingEditsForDocument(
        fileName: string,
        options: FormatCodeSettings,
    ): Promise<TextChange[]> {
        const edits = await super.getFormattingEditsForDocument(
            fileName,
            options,
        );
        return edits.filter((e) => this.inPatch(fileName, e.span));
    }

    override async getFormattingEditsForRange(
        fileName: string,
        start: number,
        end: number,
        options: FormatCodeSettings,
    ): Promise<TextChange[]> {
        const edits = await super.getFormattingEditsForRange(
            fileName,
            start,
            end,
            options,
        );
        return edits.filter((e) => this.inPatch(fileName, e.span));
    }

    override async getFormattingEditsAfterKeystroke(
        fileName: string,
        position: number,
        ch: string,
        options: FormatCodeSettings,
    ): Promise<TextChange[]> {
        const edits = await super.getFormattingEditsAfterKeystroke(
            fileName,
            position,
            ch,
            options,
        );
        return edits.filter((e) => this.inPatch(fileName, e.span));
    }

    override async provideInlayHints(
        fileName: string,
        start: number,
        end: number,
    ): Promise<readonly InlayHint[]> {
        const hints = await super.provideInlayHints(fileName, start, end);
        const view = this.view(fileName);
        return view
            ? hints.filter((hint) => hint.position < view.length)
            : hints;
    }

    /**
     * `tables` rides every call: Monaco stops an idle worker and starts a
     * fresh one, so nothing set once on the worker would last.
     */
    async getSignalSites(
        fileName: string,
        tables: SignalSiteTables,
    ): Promise<SignalSite[]> {
        const program = this.getLanguageService().getProgram();
        const sourceFile = program?.getSourceFile(fileName);
        if (!program || !sourceFile) {
            return [];
        }
        const sites = collectSignalSites(
            ts,
            sourceFile,
            program.getTypeChecker(),
            tables,
        );
        const view = this.view(fileName);
        return view ? sites.filter((site) => site.start < view.length) : sites;
    }

    async getLocalRefactors(
        fileName: string,
        start: number,
        end: number,
        invoked: boolean,
    ): Promise<LocalRefactor[]> {
        const refactors = localRefactors(
            ts,
            this.getLanguageService(),
            fileName,
            start,
            end,
            invoked,
        );
        // A track's name has no value to inline: its binding is the label.
        const view = this.view(fileName);
        const definitions =
            view &&
            this.getLanguageService().getDefinitionAtPosition(fileName, start);
        const isTrackName =
            !!view &&
            !!definitions?.length &&
            definitions.every((d) => d.textSpan.start >= view.length);
        return isTrackName
            ? refactors.filter((r) => r.kind !== 'refactor.inline.variable')
            : refactors;
    }

    async getLocalRefactorEdits(
        fileName: string,
        start: number,
        end: number,
        refactor: string,
        action: string,
        style: RefactorStyle,
    ): Promise<LocalRefactorEdits | null> {
        const result = localRefactorEdits(
            ts,
            this.getLanguageService(),
            fileName,
            start,
            end,
            refactor,
            action,
            style,
        );
        const view = this.view(fileName);
        if (!result || !view) {
            return result;
        }
        // Edits to the appended declarations land nowhere in the buffer; one
        // that reaches across the patch's end, or writes a binding into it,
        // would corrupt the patch.
        const edits = result.edits.filter((e) => e.start < view.length);
        if (
            edits.some(
                (e) =>
                    e.end > view.length || e.newText.includes('__trackBinding'),
            )
        ) {
            return null;
        }
        return { ...result, edits };
    }
}

/** The methods the renderer calls on the worker beyond Monaco's own. */
export type DslTypeScriptWorkerApi = Pick<
    DslTypeScriptWorker,
    'getSignalSites' | 'getLocalRefactors' | 'getLocalRefactorEdits'
>;

self.onmessage = () => {
    initialize((ctx, createData) => new DslTypeScriptWorker(ctx, createData));
};
