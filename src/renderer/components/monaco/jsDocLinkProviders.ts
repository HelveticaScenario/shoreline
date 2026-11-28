import type {
    IDisposable,
    IPosition,
    Uri,
    editor,
    languages,
} from 'monaco-editor';
import {
    QuickInfoAdapter,
    SignatureHelpAdapter,
    SuggestAdapter,
} from 'monaco-editor/esm/vs/language/typescript/languageFeatures.js';
import { ICommandService } from 'monaco-editor/esm/vs/platform/commands/common/commands.js';
import type {
    JSDocTagInfo,
    SignatureHelpItem,
    SymbolDisplayPart,
} from 'typescript';
import type { ModuleSchema } from '@modular/core';
import type { Monaco } from '../../hooks/useCustomMonaco';
import { type DocsTarget, SHOW_DOCS_COMMAND } from '../docsCommand';
import { executeCommand } from '../../keybindings/commands';
import { type DocsNames, docsTargetForName } from './definitionDocs';
import {
    type JsDocLinkRef,
    type JsDocLinkResolver,
    docPartsToMarkdown,
    jsDocToMarkdown,
} from './jsDocMarkdown';
import type { QuoteStyle } from '../../../shared/ipcTypes';
import { requoteMarkdown } from '../../dsl/requote';

function plainText(parts: readonly SymbolDisplayPart[] | undefined): string {
    return parts?.map((p) => p.text).join('') ?? '';
}

const OPEN_LINK_COMMAND = 'shoreline.openJsDocLink';

/** `isTrusted` scoped to the link command, so doc text cannot run others. */
const LINK_TRUST = { enabledCommands: [OPEN_LINK_COMMAND] };

type LinkAction =
    | ({ kind: 'docs' } & DocsTarget)
    | {
          kind: 'peek';
          from: string;
          position: IPosition;
          file: string;
          start: number;
          length: number;
      };

/**
 * Resolves links shown at `position` in `model`: DSL types, modules and
 * globals open in the Docs panel, any other declaration opens in a peek at
 * `position`.
 */
function linkResolver(
    names: DocsNames,
    from: Uri,
    position: IPosition,
): JsDocLinkResolver {
    return ({ name, target }: JsDocLinkRef) => {
        const docs = docsTargetForName(name, names);
        let action: LinkAction | undefined;
        if (docs) {
            action = { kind: 'docs', ...docs };
        } else if (target) {
            action = {
                file: target.fileName,
                from: from.toString(),
                kind: 'peek',
                length: target.textSpan.length,
                position: {
                    column: position.column,
                    lineNumber: position.lineNumber,
                },
                start: target.textSpan.start,
            };
        }
        return (
            action &&
            `command:${OPEN_LINK_COMMAND}?${encodeURIComponent(JSON.stringify([action]))}`
        );
    };
}

function openLink(
    monaco: Monaco,
    commands: ICommandService,
    action: LinkAction,
) {
    if (action.kind === 'docs') {
        const { name, symbolType } = action;
        executeCommand(SHOW_DOCS_COMMAND, { name, symbolType });
        return;
    }
    const target = monaco.editor.getModel(monaco.Uri.parse(action.file));
    if (!target) {
        return;
    }
    const range = monaco.Range.fromPositions(
        target.getPositionAt(action.start),
        target.getPositionAt(action.start + action.length),
    );
    void commands.executeCommand(
        'editor.action.peekLocations',
        monaco.Uri.parse(action.from),
        new monaco.Position(action.position.lineNumber, action.position.column),
        [{ range, uri: target.uri }],
        'peek',
    );
}

/**
 * Hover, completion details and signature help for the JavaScript language
 * with working `{@link}`s. They replace the bundled TypeScript providers,
 * which print a link as its raw `{@link Name}` source text. Code examples
 * in the docs follow `quotes`, read each time one is shown.
 */
export function registerJsDocLinkProviders(
    monaco: Monaco,
    schemas: readonly ModuleSchema[],
    names: DocsNames,
    quotes: () => QuoteStyle,
): IDisposable {
    const ts = monaco.typescript;
    const defaults = ts.javascriptDefaults;
    const previousModes = defaults.modeConfiguration;
    defaults.setModeConfiguration({
        ...previousModes,
        completionItems: false,
        hovers: false,
        signatureHelp: false,
    });

    // Completion labels an experimental module can appear under: its factory
    // name and its chain-method name.
    const experimentalLabels = new Set(
        schemas
            .filter((s) => s.experimental)
            .flatMap((s) => {
                const leaf = s.name.split('.').pop() ?? s.name;
                return [leaf, leaf.replace(/^\$/, '')];
            }),
    );
    const resolverAt = (from: Uri, position: IPosition) =>
        linkResolver(names, from, position);
    const docs = (markdown: string) => ({
        isTrusted: LINK_TRUST,
        value: requoteMarkdown(markdown, quotes()),
    });

    const worker = async (...uris: Uri[]) =>
        (await ts.getJavaScriptWorker())(...uris);

    class LinkedQuickInfoAdapter extends QuickInfoAdapter {
        override async provideHover(
            model: editor.ITextModel,
            position: IPosition,
        ) {
            const resource = model.uri;
            const languageService = await this._worker(resource);
            if (model.isDisposed()) {
                return undefined;
            }
            const info = await languageService.getQuickInfoAtPosition(
                resource.toString(),
                model.getOffsetAt(position),
            );
            if (!info || model.isDisposed()) {
                return undefined;
            }
            const signature = plainText(info.displayParts);
            return {
                contents: [
                    { value: '```typescript\n' + signature + '\n```\n' },
                    docs(
                        jsDocToMarkdown(
                            info.documentation,
                            info.tags,
                            resolverAt(resource, position),
                        ),
                    ),
                ],
                range: this._textSpanToRange(model, info.textSpan),
            };
        }
    }

    class LinkedSuggestAdapter extends SuggestAdapter {
        /**
         * Labels an experimental module's entries in the list. A label match
         * is confirmed by the `@experimental` tag, so a user identifier that
         * shares the name stays unmarked.
         */
        override async provideCompletionItems(
            model: editor.ITextModel,
            position: IPosition,
        ) {
            const list = await super.provideCompletionItems(model, position);
            const candidates = list?.suggestions.filter(
                (item) =>
                    typeof item.label === 'string' &&
                    experimentalLabels.has(item.label),
            );
            if (!candidates?.length) {
                return list;
            }
            const languageService = await this._worker(model.uri);
            if (model.isDisposed()) {
                return list;
            }
            await Promise.all(
                candidates.map(async (item) => {
                    const name = item.label as string;
                    const details =
                        await languageService.getCompletionEntryDetails(
                            model.uri.toString(),
                            model.getOffsetAt(position),
                            name,
                        );
                    if (
                        details?.tags?.some(
                            (tag: JSDocTagInfo) => tag.name === 'experimental',
                        )
                    ) {
                        item.label = {
                            description: 'experimental',
                            label: name,
                        };
                    }
                }),
            );
            return list;
        }

        override async resolveCompletionItem(item: languages.CompletionItem) {
            const { uri, position, offset } =
                item as languages.CompletionItem & {
                    uri: Uri;
                    position: IPosition;
                    offset: number;
                };
            const name =
                typeof item.label === 'string' ? item.label : item.label.label;
            const languageService = await this._worker(uri);
            const details = await languageService.getCompletionEntryDetails(
                uri.toString(),
                offset,
                name,
            );
            if (!details) {
                return item;
            }
            return {
                ...item,
                detail: plainText(details.displayParts),
                documentation: docs(
                    jsDocToMarkdown(
                        details.documentation,
                        details.tags,
                        resolverAt(uri, position),
                    ),
                ),
                kind: SuggestAdapter.convertKind(details.kind),
                label:
                    typeof item.label === 'string' ? details.name : item.label,
            };
        }
    }

    class LinkedSignatureHelpAdapter extends SignatureHelpAdapter {
        override async provideSignatureHelp(
            model: editor.ITextModel,
            position: IPosition,
            _token: unknown,
            context: languages.SignatureHelpContext,
        ) {
            const languageService = await this._worker(model.uri);
            if (model.isDisposed()) {
                return undefined;
            }
            const info = await languageService.getSignatureHelpItems(
                model.uri.toString(),
                model.getOffsetAt(position),
                {
                    triggerReason:
                        SignatureHelpAdapter._toSignatureHelpTriggerReason(
                            context,
                        ),
                },
            );
            if (!info || model.isDisposed()) {
                return undefined;
            }
            const resolve = resolverAt(model.uri, position);
            const signatures = info.items.map((item: SignatureHelpItem) => {
                const parameters = item.parameters.map((p) => ({
                    documentation: docs(
                        docPartsToMarkdown(p.documentation, resolve),
                    ),
                    label: plainText(p.displayParts),
                }));
                const label =
                    plainText(item.prefixDisplayParts) +
                    parameters
                        .map((p) => p.label)
                        .join(plainText(item.separatorDisplayParts)) +
                    plainText(item.suffixDisplayParts);
                return {
                    documentation: docs(
                        docPartsToMarkdown(item.documentation, resolve),
                    ),
                    label,
                    parameters,
                };
            });
            return {
                dispose() {},
                value: {
                    activeParameter: info.argumentIndex,
                    activeSignature: info.selectedItemIndex,
                    signatures,
                },
            };
        }
    }

    const disposables = [
        monaco.editor.registerCommand(
            OPEN_LINK_COMMAND,
            (accessor, action: LinkAction) =>
                openLink(monaco, accessor.get(ICommandService), action),
        ),
        monaco.languages.registerHoverProvider(
            'javascript',
            new LinkedQuickInfoAdapter(worker),
        ),
        monaco.languages.registerCompletionItemProvider(
            'javascript',
            new LinkedSuggestAdapter(worker),
        ),
        monaco.languages.registerSignatureHelpProvider(
            'javascript',
            new LinkedSignatureHelpAdapter(worker),
        ),
    ];

    return {
        dispose() {
            for (const d of disposables) {
                d.dispose();
            }
            defaults.setModeConfiguration(previousModes);
        },
    };
}
