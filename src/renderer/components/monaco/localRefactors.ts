/**
 * TypeScript's extract and inline refactors, limited to those that edit only
 * the buffer they run in. Runs in the TypeScript worker (dslTsWorker.ts)
 * against the language service the editor keeps warm.
 */
import type * as TS from 'typescript';

/** Refactor kinds that stay within the edited buffer. */
export const LOCAL_REFACTOR_KINDS = [
    'refactor.extract.constant',
    'refactor.extract.function',
    'refactor.inline.variable',
];

/** One refactor TypeScript offers over a range. */
export interface LocalRefactor {
    refactor: string;
    action: string;
    title: string;
    kind: string;
}

/** A refactor's edits to the buffer, and where the name it introduced
 *  lands once they apply. */
export interface LocalRefactorEdits {
    edits: { start: number; end: number; newText: string }[];
    renameOffset?: number;
}

/** The options a refactor's generated code follows. */
export interface RefactorStyle {
    tabSize: number;
    convertTabsToSpaces: boolean;
    singleQuote: boolean;
    semi: boolean;
}

/**
 * The extract and inline refactors for [start, end) of `fileName`.
 * `invoked` is an explicit request, for which an empty range extracts the
 * expression at the cursor.
 */
export function localRefactors(
    service: TS.LanguageService,
    fileName: string,
    start: number,
    end: number,
    invoked: boolean,
): LocalRefactor[] {
    let refactors: readonly TS.ApplicableRefactorInfo[];
    try {
        refactors = service.getApplicableRefactors(
            fileName,
            { end, pos: start },
            {},
            invoked ? 'invoked' : 'implicit',
        );
    } catch {
        return [];
    }
    // Inlining deletes the declaration, which a use in another buffer
    // still needs: a top-level patch variable is global to every open
    // buffer, and the refactor only rewrites uses in its own.
    const usedElsewhere = () =>
        (service.getReferencesAtPosition(fileName, start) ?? []).some(
            (ref) => ref.fileName !== fileName,
        );
    return refactors.flatMap((r) =>
        r.actions.flatMap((a) =>
            !a.notApplicableReason &&
            a.kind !== undefined &&
            LOCAL_REFACTOR_KINDS.includes(a.kind) &&
            !(a.kind === 'refactor.inline.variable' && usedElsewhere())
                ? [
                      {
                          action: a.name,
                          kind: a.kind,
                          refactor: r.name,
                          title: a.description,
                      },
                  ]
                : [],
        ),
    );
}

/** The edits applying `action` of `refactor` over [start, end), or null
 *  when they would touch another file. */
export function localRefactorEdits(
    ts: typeof TS,
    service: TS.LanguageService,
    fileName: string,
    start: number,
    end: number,
    refactor: string,
    action: string,
    style: RefactorStyle,
): LocalRefactorEdits | null {
    const format: TS.FormatCodeSettings = {
        ...ts.getDefaultFormatCodeSettings(),
        convertTabsToSpaces: style.convertTabsToSpaces,
        indentSize: style.tabSize,
        semicolons: style.semi
            ? ts.SemicolonPreference.Insert
            : ts.SemicolonPreference.Remove,
        tabSize: style.tabSize,
    };
    let info: TS.RefactorEditInfo | undefined;
    try {
        info = service.getEditsForRefactor(
            fileName,
            format,
            { end, pos: start },
            refactor,
            action,
            { quotePreference: style.singleQuote ? 'single' : 'double' },
        );
    } catch {
        return null;
    }
    if (
        !info ||
        info.edits.some((e) => e.fileName !== fileName || e.isNewFile)
    ) {
        return null;
    }
    return {
        edits: info.edits.flatMap((e) =>
            e.textChanges.map((c) => ({
                end: c.span.start + c.span.length,
                newText: c.newText,
                start: c.span.start,
            })),
        ),
        renameOffset:
            info.renameFilename === fileName ? info.renameLocation : undefined,
    };
}
