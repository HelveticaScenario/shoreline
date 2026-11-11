/**
 * TypeScript's extract (to constant, to function) and inline-variable
 * refactors as Monaco code actions on patch buffers. An extract leaves the
 * new name in rename mode, so it can be named straight away.
 */
import type { editor, languages } from 'monaco-editor';

import type { Monaco } from '../../hooks/useCustomMonaco';
import type { PrettierConfig } from '../../../shared/ipcTypes';
import type { DslTypeScriptWorkerApi } from './dslTsWorker';
import { LOCAL_REFACTOR_KINDS } from './localRefactors';
import type { LocalRefactor } from './localRefactors';

const RENAME_CMD = 'modular.renameAfterRefactor';

/** Where the introduced name lands once the edit applies; null until the
 *  action resolves, or when it introduces none. */
interface RenameTarget {
    uri: string;
    offset: number | null;
}

interface RefactorCodeAction extends languages.CodeAction {
    refactor: LocalRefactor;
    model: editor.ITextModel;
    versionId: number;
    start: number;
    end: number;
    rename: RenameTarget;
}

export function registerRefactorProvider(
    monaco: Monaco,
    getFormatOptions: () => PrettierConfig,
): { dispose: () => void } {
    const workerFor = async (model: editor.ITextModel) => {
        const getWorker = await monaco.typescript.getJavaScriptWorker();
        return (await getWorker(
            model.uri,
        )) as unknown as DslTypeScriptWorkerApi;
    };

    // Runs after Monaco applies the edit: start renaming the new name.
    const commandDisposable = monaco.editor.registerCommand(
        RENAME_CMD,
        (_accessor: unknown, target: RenameTarget) => {
            const ed = monaco.editor
                .getEditors()
                .find((e) => e.getModel()?.uri.toString() === target.uri);
            const model = ed?.getModel();
            if (!ed || !model || target.offset === null) {
                return;
            }
            const position = model.getPositionAt(target.offset);
            ed.setPosition(position);
            ed.revealPositionInCenterIfOutsideViewport(position);
            ed.focus();
            void ed.getAction('editor.action.rename')?.run();
        },
    );

    const provider: languages.CodeActionProvider = {
        async provideCodeActions(model, range, context, token) {
            const empty = { actions: [], dispose() {} };
            const path = model.uri.path;
            if (!path.endsWith('.js') && !path.endsWith('.mjs')) {
                return empty;
            }
            if (context.only && !context.only.startsWith('refactor')) {
                return empty;
            }
            const start = model.getOffsetAt(range.getStartPosition());
            const end = model.getOffsetAt(range.getEndPosition());
            const versionId = model.getVersionId();
            const worker = await workerFor(model);
            const refactors = await worker.getLocalRefactors(
                model.uri.toString(),
                start,
                end,
                context.trigger ===
                    monaco.languages.CodeActionTriggerType.Invoke,
            );
            if (token.isCancellationRequested || model.isDisposed()) {
                return empty;
            }
            // Monaco resolves an action only while it has no edit, so the
            // edit is left for resolveCodeAction to fill in.
            const actions = refactors.map((refactor): RefactorCodeAction => {
                const rename: RenameTarget = {
                    offset: null,
                    uri: model.uri.toString(),
                };
                return {
                    command: refactor.kind.startsWith('refactor.extract')
                        ? {
                              arguments: [rename],
                              id: RENAME_CMD,
                              title: 'Rename extracted symbol',
                          }
                        : undefined,
                    end,
                    kind: refactor.kind,
                    model,
                    refactor,
                    rename,
                    start,
                    title: refactor.title,
                    versionId,
                };
            });
            return { actions, dispose() {} };
        },

        async resolveCodeAction(codeAction) {
            const action = codeAction as RefactorCodeAction;
            const { model, refactor } = action;
            if (
                model.isDisposed() ||
                model.getVersionId() !== action.versionId
            ) {
                return codeAction;
            }
            const options = model.getOptions();
            const format = getFormatOptions();
            const worker = await workerFor(model);
            const result = await worker.getLocalRefactorEdits(
                model.uri.toString(),
                action.start,
                action.end,
                refactor.refactor,
                refactor.action,
                {
                    convertTabsToSpaces: options.insertSpaces,
                    semi: format.semi ?? true,
                    singleQuote: format.singleQuote ?? true,
                    tabSize: options.tabSize,
                },
            );
            if (!result || model.getVersionId() !== action.versionId) {
                return codeAction;
            }
            action.rename.offset = result.renameOffset ?? null;
            return {
                ...codeAction,
                edit: {
                    edits: result.edits.map((e) => ({
                        resource: model.uri,
                        textEdit: {
                            range: monaco.Range.fromPositions(
                                model.getPositionAt(e.start),
                                model.getPositionAt(e.end),
                            ),
                            text: e.newText,
                        },
                        versionId: action.versionId,
                    })),
                },
            };
        },
    };

    const providerDisposable = monaco.languages.registerCodeActionProvider(
        'javascript',
        provider,
        {
            providedCodeActionKinds: LOCAL_REFACTOR_KINDS,
        },
    );

    return {
        dispose: () => {
            providerDisposable.dispose();
            commandDisposable.dispose();
        },
    };
}
