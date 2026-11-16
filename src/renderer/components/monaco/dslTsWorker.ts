/**
 * Monaco's TypeScript worker, extended with the DSL's signal-site collector
 * so output-resolution hints reuse the program the editor already keeps warm,
 * with the extract and inline refactors Monaco's code actions leave out,
 * with a plain diagnostic for `$'…'` in place of the parser's, and with the
 * names track labels bind (see trackLabelBindings).
 */

import type {
    DefinitionInfo,
    Diagnostic,
    DocumentHighlights,
    InlayHint,
    ReferenceEntry,
    RenameLocation,
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
    mapSpan,
    trackLabelView,
    withinPatch,
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
        return withinPatch(
            view,
            withoutDollarQuoteTypeErrors(ts, sourceFile, diagnostics),
        );
    }

    override async getSuggestionDiagnostics(
        fileName: string,
    ): Promise<Diagnostic[]> {
        const diagnostics = await super.getSuggestionDiagnostics(fileName);
        const view = this.view(fileName);
        return view ? withinPatch(view, diagnostics) : diagnostics;
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
            await super.getDefinitionAtPosition(fileName, position),
        );
    }

    override async getReferencesAtPosition(
        fileName: string,
        position: number,
    ): Promise<ReferenceEntry[] | undefined> {
        return this.mapEntries(
            await super.getReferencesAtPosition(fileName, position),
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
                position,
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
            position,
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
        return localRefactors(
            ts,
            this.getLanguageService(),
            fileName,
            start,
            end,
            invoked,
        );
    }

    async getLocalRefactorEdits(
        fileName: string,
        start: number,
        end: number,
        refactor: string,
        action: string,
        style: RefactorStyle,
    ): Promise<LocalRefactorEdits | null> {
        return localRefactorEdits(
            ts,
            this.getLanguageService(),
            fileName,
            start,
            end,
            refactor,
            action,
            style,
        );
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
