/**
 * Rename Symbol on patch buffers. It replaces the bundled TypeScript
 * provider, which offers the whole word under the cursor as the name to
 * edit: on a track label that word carries the label's mute/solo prefix and
 * channel, which are not part of the track's name.
 */
import type { editor, IDisposable, IPosition, languages } from 'monaco-editor';

import type { Monaco } from '../../hooks/useCustomMonaco';
import { dslWorker } from './dslWorker';

/** The editor contribution that shows a short message at a position. */
interface MessageController extends editor.IEditorContribution {
    showMessage(message: string, position: IPosition): void;
}

export function registerRenameProvider(monaco: Monaco): IDisposable {
    const ts = monaco.typescript;
    const defaults = ts.javascriptDefaults;
    const previousModes = defaults.modeConfiguration;
    defaults.setModeConfiguration({ ...previousModes, rename: false });

    const workerFor = (model: editor.ITextModel) =>
        dslWorker(monaco, model.uri);

    const rangeOf = (
        model: editor.ITextModel,
        span: { start: number; length: number },
    ) =>
        monaco.Range.fromPositions(
            model.getPositionAt(span.start),
            model.getPositionAt(span.start + span.length),
        );

    // Monaco's standalone build shows nothing for a rename its provider
    // rejects once the new name is typed, so the reason is shown here, at
    // the cursor.
    const reject = (
        model: editor.ITextModel,
        position: IPosition,
        rejectReason: string,
    ): languages.WorkspaceEdit & languages.Rejection => {
        monaco.editor
            .getEditors()
            .find((e) => e.getModel() === model)
            ?.getContribution<MessageController>(
                'editor.contrib.messageController',
            )
            ?.showMessage(rejectReason, position);
        return { edits: [], rejectReason };
    };

    const provider: languages.RenameProvider = {
        async provideRenameEdits(model, position, newName) {
            const fileName = model.uri.toString();
            const offset = model.getOffsetAt(position);
            const worker = await workerFor(model);
            const error = await worker.getTrackRenameError(
                fileName,
                offset,
                newName,
            );
            if (error !== null) {
                return reject(model, position, error);
            }
            const locations = await worker.findRenameLocations(
                fileName,
                offset,
                false,
                false,
                false,
            );
            if (!locations || model.isDisposed()) {
                return undefined;
            }
            // A name declared outside the patch is the DSL's own.
            if (locations.some((l) => l.fileName !== fileName)) {
                return reject(
                    model,
                    position,
                    'Only a name the patch declares can be renamed',
                );
            }
            return {
                edits: locations.map((l) => ({
                    resource: model.uri,
                    textEdit: {
                        range: rangeOf(model, l.textSpan),
                        text: newName,
                    },
                    versionId: undefined,
                })),
            };
        },

        async resolveRenameLocation(model, position) {
            const worker = await workerFor(model);
            const info = await worker.getRenameInfo(
                model.uri.toString(),
                model.getOffsetAt(position),
                { allowRenameOfImportPath: false },
            );
            if (!info.canRename) {
                return {
                    range: monaco.Range.fromPositions(position),
                    rejectReason: info.localizedErrorMessage,
                    text: '',
                };
            }
            const range = rangeOf(model, info.triggerSpan);
            return { range, text: model.getValueInRange(range) };
        },
    };

    const registration = monaco.languages.registerRenameProvider(
        'javascript',
        provider,
    );
    return {
        dispose() {
            registration.dispose();
            defaults.setModeConfiguration(previousModes);
        },
    };
}
