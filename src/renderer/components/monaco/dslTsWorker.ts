/**
 * Monaco's TypeScript worker, extended with the DSL's signal-site collector
 * so output-resolution hints reuse the program the editor already keeps warm,
 * with the extract and inline refactors Monaco's code actions leave out, and
 * with a plain diagnostic for `$'…'` in place of the parser's.
 */

import type { Diagnostic } from 'typescript';
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

class DslTypeScriptWorker extends TypeScriptWorker {
    override async getSyntacticDiagnostics(
        fileName: string,
    ): Promise<Diagnostic[]> {
        const sourceFile = this.getLanguageService()
            .getProgram()
            ?.getSourceFile(fileName);
        const diagnostics = await super.getSyntacticDiagnostics(fileName);
        return sourceFile
            ? TypeScriptWorker.clearFiles(
                  withDollarQuoteDiagnostics(ts, sourceFile, diagnostics),
              )
            : diagnostics;
    }

    override async getSemanticDiagnostics(
        fileName: string,
    ): Promise<Diagnostic[]> {
        const sourceFile = this.getLanguageService()
            .getProgram()
            ?.getSourceFile(fileName);
        const diagnostics = await super.getSemanticDiagnostics(fileName);
        return sourceFile
            ? withoutDollarQuoteTypeErrors(ts, sourceFile, diagnostics)
            : diagnostics;
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
        return collectSignalSites(
            ts,
            sourceFile,
            program.getTypeChecker(),
            tables,
        );
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
