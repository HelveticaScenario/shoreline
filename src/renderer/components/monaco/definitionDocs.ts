/**
 * Go to Definition on a name the Docs panel covers opens its docs: the DSL
 * lib that declares it is no file a musician reads.
 */

import type { IDisposable, IPosition, editor } from 'monaco-editor';
import type { ModuleSchema } from '@modular/core';
import type { Monaco } from '../../hooks/useCustomMonaco';
import { type DocsTarget, SHOW_DOCS_COMMAND } from '../docsCommand';
import { executeCommand } from '../../keybindings/commands';
import { isDslType } from '../../../shared/dsl/typeDocs';
import { GLOBAL_ENTRIES } from '../../../shared/dsl/globalDocs';
import { dslModules } from '../../../shared/dsl/dslModuleDocs';
import { functionName } from '../../../shared/dsl/referenceDocs';
import { dslWorker } from './dslWorker';

/** The names the Docs panel has a page or card for. */
export interface DocsNames {
    modules: ReadonlySet<string>;
    /** Module namespaces, such as `$mi` and `$unstable.shape`. */
    namespaces: ReadonlySet<string>;
}

const GLOBAL_NAMES: ReadonlySet<string> = new Set(
    GLOBAL_ENTRIES.map((e) => e.name),
);

export function docsNames(schemas: readonly ModuleSchema[]): DocsNames {
    return {
        modules: new Set([
            ...schemas.map((s) => s.name),
            ...dslModules(schemas).map(functionName),
        ]),
        namespaces: new Set(
            schemas.flatMap((s) => {
                const segments = s.name.split('.');
                return segments
                    .slice(1)
                    .map((_, i) => segments.slice(0, i + 1).join('.'));
            }),
        ),
    };
}

/** The docs of `name` as the DSL writes it (`$sine`, `$mi`, `Pattern`). */
export function docsTargetForName(
    name: string,
    names: DocsNames,
): DocsTarget | null {
    if (isDslType(name)) {
        return { name, symbolType: 'type' };
    }
    if (names.modules.has(name)) {
        return { name, symbolType: 'module' };
    }
    if (names.namespaces.has(name)) {
        return { name, symbolType: 'namespace' };
    }
    if (GLOBAL_NAMES.has(name)) {
        return { name, symbolType: 'global' };
    }
    return null;
}

/** As the DSL lib names its chain interfaces: `$unstable.shape` is `UnstableShape`. */
const pascal = (path: string): string =>
    path
        .split(/[^a-zA-Z0-9]/)
        .filter(Boolean)
        .map((w) => w[0].toUpperCase() + w.slice(1))
        .join('');

const CHAIN = 'ModuleChain';

/**
 * The docs of the lib declaration TypeScript names `qualified`
 * (`$sine`, `ModuleChainMi.chorus`, `Pattern.fast`): a module, namespace,
 * global or type, a chain method's module, or a type's own method's type.
 */
export function docsTargetForDeclaration(
    qualified: string,
    names: DocsNames,
): DocsTarget | null {
    const direct = docsTargetForName(qualified, names);
    if (direct) {
        return direct;
    }
    const dot = qualified.lastIndexOf('.');
    if (dot < 0) {
        return null;
    }
    const container = qualified.slice(0, dot);
    const member = qualified.slice(dot + 1);
    if (container.startsWith(CHAIN)) {
        const path = container.slice(CHAIN.length);
        const namespace =
            path === ''
                ? ''
                : [...names.namespaces].find((ns) => pascal(ns) === path);
        return namespace === undefined
            ? null
            : docsTargetForName(
                  namespace ? `${namespace}.${member}` : `$${member}`,
                  names,
              );
    }
    return (
        docsTargetForName(`$${member}`, names) ??
        docsTargetForName(container, names)
    );
}

/** The registered opener's state; null until it is registered. */
let active: { monaco: Monaco; names: DocsNames } | null = null;

/** The docs of the lib declaration at `offset` in `lib`, if it has them. */
async function declarationDocs(
    lib: editor.ITextModel,
    offset: number,
): Promise<DocsTarget | null> {
    if (!active) {
        return null;
    }
    const worker = await dslWorker(active.monaco, lib.uri);
    const qualified = await worker.getDeclarationName(
        lib.uri.toString(),
        offset,
    );
    return qualified && active
        ? docsTargetForDeclaration(qualified, active.names)
        : null;
}

/** Whether the name at `position` in `model` is declared where docs cover it. */
export async function hasDefinitionDocs(
    model: editor.ITextModel,
    position: IPosition,
): Promise<boolean> {
    if (!active) {
        return false;
    }
    const { monaco } = active;
    const worker = await dslWorker(monaco, model.uri);
    if (model.isDisposed()) {
        return false;
    }
    const [definition] =
        (await worker.getDefinitionAtPosition(
            model.uri.toString(),
            model.getOffsetAt(position),
        )) ?? [];
    const lib =
        definition?.fileName.endsWith('.d.ts') &&
        monaco.editor.getModel(monaco.Uri.parse(definition.fileName));
    return (
        !!lib &&
        (await declarationDocs(lib, definition.textSpan.start)) !== null
    );
}

/**
 * Opens the docs for a definition in the DSL lib that has them, wherever Go
 * to Definition is asked.
 */
export function registerDefinitionDocs(
    monaco: Monaco,
    names: DocsNames,
): IDisposable {
    active = { monaco, names };
    const opener = monaco.editor.registerEditorOpener({
        async openCodeEditor(_source, resource, selectionOrPosition) {
            const lib = monaco.editor.getModel(resource);
            if (
                !lib ||
                !resource.path.endsWith('.d.ts') ||
                !selectionOrPosition
            ) {
                return false;
            }
            const start =
                'startLineNumber' in selectionOrPosition
                    ? {
                          column: selectionOrPosition.startColumn,
                          lineNumber: selectionOrPosition.startLineNumber,
                      }
                    : selectionOrPosition;
            const target = await declarationDocs(lib, lib.getOffsetAt(start));
            if (!target) {
                return false;
            }
            executeCommand(SHOW_DOCS_COMMAND, target);
            return true;
        },
    });
    return {
        dispose() {
            opener.dispose();
            active = null;
        },
    };
}
