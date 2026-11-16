import type { IDisposable, languages } from 'monaco-editor';
import type { Monaco } from '../../hooks/useCustomMonaco';
import { backtickedPattern, DOLLAR_QUOTE_CODE } from './dollarQuotes';

/** A quick fix rewriting each `$'…'` diagnostic's text with backticks. */
export function registerDollarQuoteQuickFix(monaco: Monaco): IDisposable {
    const code = String(DOLLAR_QUOTE_CODE);
    return monaco.languages.registerCodeActionProvider(
        'javascript',
        {
            provideCodeActions(model, _range, context) {
                const actions: languages.CodeAction[] = [];
                for (const marker of context.markers) {
                    const text =
                        marker.code === code
                            ? backtickedPattern(model.getValueInRange(marker))
                            : null;
                    if (text === null) {
                        continue;
                    }
                    actions.push({
                        diagnostics: [marker],
                        edit: {
                            edits: [
                                {
                                    resource: model.uri,
                                    textEdit: { range: marker, text },
                                    versionId: model.getVersionId(),
                                },
                            ],
                        },
                        isPreferred: true,
                        kind: 'quickfix',
                        title: 'Replace quotes with backticks',
                    });
                }
                return { actions, dispose: () => {} };
            },
        },
        { providedCodeActionKinds: ['quickfix'] },
    );
}
