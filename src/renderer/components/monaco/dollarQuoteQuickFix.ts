import type { IDisposable, languages } from 'monaco-editor';
import type { Monaco } from '../../hooks/useCustomMonaco';
import { DOLLAR_QUOTE_CODE, templateBody } from './dollarQuotes';

/** A quick fix rewriting each `$'…'` diagnostic's text with backticks. */
export function registerDollarQuoteQuickFix(monaco: Monaco): IDisposable {
    const code = String(DOLLAR_QUOTE_CODE);
    return monaco.languages.registerCodeActionProvider(
        'javascript',
        {
            provideCodeActions(model, _range, context) {
                const actions: languages.CodeAction[] = context.markers
                    .filter((marker) => marker.code === code)
                    .map((marker) => {
                        const written = model.getValueInRange(marker);
                        return {
                            diagnostics: [marker],
                            edit: {
                                edits: [
                                    {
                                        resource: model.uri,
                                        textEdit: {
                                            range: marker,
                                            text: `$\`${templateBody(written.slice(2, -1))}\``,
                                        },
                                        versionId: model.getVersionId(),
                                    },
                                ],
                            },
                            isPreferred: true,
                            kind: 'quickfix',
                            title: 'Replace quotes with backticks',
                        };
                    });
                return { actions, dispose: () => {} };
            },
        },
        { providedCodeActionKinds: ['quickfix'] },
    );
}
