/**
 * Monaco's TypeScript worker, extended with the DSL's signal-site collector
 * so output-resolution and fold hints reuse the program the editor already
 * keeps warm,
 * and with the extract and inline refactors Monaco's code actions leave out.
 */

import { initialize } from 'monaco-editor/esm/vs/common/initialize.js';
import { typescript as ts } from 'monaco-editor/esm/vs/language/typescript/lib/typescriptServices.js';
import { TypeScriptWorker } from 'monaco-editor/esm/vs/language/typescript/tsWorker.js';
import {
    collectSignalSites,
    collectValueSites,
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

class DslTypeScriptWorker extends TypeScriptWorker {
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

    /** Every signal value, for the fold hints. */
    async getValueSites(
        fileName: string,
        tables: SignalSiteTables,
    ): Promise<SignalSite[]> {
        const program = this.getLanguageService().getProgram();
        const sourceFile = program?.getSourceFile(fileName);
        if (!program || !sourceFile) {
            return [];
        }
        return collectValueSites(
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
    | 'getSignalSites'
    | 'getValueSites'
    | 'getLocalRefactors'
    | 'getLocalRefactorEdits'
>;

self.onmessage = () => {
    initialize((ctx, createData) => new DslTypeScriptWorker(ctx, createData));
};
