/**
 * Monaco quick-fix provider that turns DSL params into control calls
 * (`$slider`, `$btn`, `$toggleBtn`). Analysis lives in
 * src/renderer/dsl/controlQuickFix.ts and formatting in
 * src/renderer/dsl/controlFixFormat.ts, which runs only for the action the
 * user picks.
 */
import type { Monaco } from '../../hooks/useCustomMonaco';
import type { editor, languages } from 'monaco-editor';

import type { ControlApi, ControlQuickFix } from '../../dsl/controlQuickFix';
import { computeControlQuickFixes } from '../../dsl/controlQuickFix';
import { formatControlQuickFix } from '../../dsl/controlFixFormat';
import type { FormatOptions } from '../../dsl/controlFixFormat';

const CURSOR_CMD = 'modular.placeCursorAfterControl';

/** Buffers larger than this skip analysis — a full parse per lightbulb
 *  request is only cheap for patch-sized sources. */
const MAX_ANALYZED_SOURCE_LENGTH = 100_000;

/** Where the caret goes once the action's edit applies. The command gets
 *  this object itself, so the offset resolving the action writes is the one
 *  it reads; null until then, so an action applied without an edit leaves
 *  the caret alone. */
interface CaretTarget {
    uri: string;
    offset: number | null;
}

interface ControlCodeAction extends languages.CodeAction {
    source: string;
    fix: ControlQuickFix;
    model: editor.ITextModel;
    versionId: number;
    caret: CaretTarget;
}

export function registerControlQuickFixProvider(
    monaco: Monaco,
    getApi: () => ControlApi,
    getFormatOptions: () => FormatOptions,
): { dispose: () => void } {
    // Runs after Monaco applies a code action's edit: put the caret just
    // past the inserted control call.
    const commandDisposable = monaco.editor.registerCommand(
        CURSOR_CMD,
        (_accessor: unknown, caret: CaretTarget) => {
            const target = monaco.editor
                .getEditors()
                .find((e) => e.getModel()?.uri.toString() === caret.uri);
            const model = target?.getModel();
            if (!target || !model || caret.offset === null) {
                return;
            }
            const position = model.getPositionAt(caret.offset);
            target.setPosition(position);
            target.revealPositionInCenterIfOutsideViewport(position);
            target.focus();
        },
    );

    /** Returns the workspace edit applying `fix`, and points the caret just
     *  past its control: the edit replaces [span.start, span.end), so text
     *  before span.start keeps its offsets. */
    const workspaceEdit = (
        action: ControlCodeAction,
        fix: ControlQuickFix,
    ): languages.WorkspaceEdit => {
        const { model } = action;
        action.caret.offset = fix.span.start + fix.caretOffset;
        return {
            edits: [
                {
                    resource: model.uri,
                    textEdit: {
                        range: monaco.Range.fromPositions(
                            model.getPositionAt(fix.span.start),
                            model.getPositionAt(fix.span.end),
                        ),
                        text: fix.newText,
                    },
                    versionId: action.versionId,
                },
            ],
        };
    };

    const provider: languages.CodeActionProvider = {
        provideCodeActions(
            model: editor.ITextModel,
            range,
            _context,
            token,
        ): languages.CodeActionList {
            const empty = { actions: [], dispose() {} };
            // Patch buffers only — keybindings/config are json models, and
            // the DSL lib extra model is a .d.ts.
            const path = model.uri.path;
            if (!path.endsWith('.js') && !path.endsWith('.mjs')) {
                return empty;
            }
            const api = getApi();
            if (api.schemas.length === 0) {
                return empty;
            }
            if (model.getValueLength() > MAX_ANALYZED_SOURCE_LENGTH) {
                return empty;
            }

            const source = model.getValue();
            const offset = model.getOffsetAt(range.getStartPosition());
            const quote = getFormatOptions().singleQuote === true ? "'" : '"';
            const fixes = computeControlQuickFixes(source, offset, api, quote);
            if (token.isCancellationRequested) {
                return empty;
            }

            // Monaco resolves an action only while it has no edit, so the
            // edit is left for resolveCodeAction to fill in.
            const versionId = model.getVersionId();
            const actions = fixes.map((fix): ControlCodeAction => {
                const caret: CaretTarget = {
                    offset: null,
                    uri: model.uri.toString(),
                };
                return {
                    caret,
                    command: {
                        arguments: [caret],
                        id: CURSOR_CMD,
                        title: 'Place cursor after control',
                    },
                    fix,
                    isPreferred: fix.preferred ?? false,
                    kind: 'quickfix',
                    model,
                    source,
                    title: fix.title,
                    versionId,
                };
            });
            return { actions, dispose() {} };
        },

        async resolveCodeAction(codeAction) {
            const action = codeAction as ControlCodeAction;
            if (action.model.getVersionId() !== action.versionId) {
                return codeAction;
            }
            const fix = await formatControlQuickFix(
                action.source,
                action.fix,
                getFormatOptions(),
            );
            return { ...codeAction, edit: workspaceEdit(action, fix) };
        },
    };

    const providerDisposable = monaco.languages.registerCodeActionProvider(
        'javascript',
        provider,
        { providedCodeActionKinds: ['quickfix'] },
    );

    return {
        dispose: () => {
            providerDisposable.dispose();
            commandDisposable.dispose();
        },
    };
}
