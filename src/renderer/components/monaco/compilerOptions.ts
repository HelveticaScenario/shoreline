import type * as TS from 'typescript';

/**
 * The options the editor's TypeScript worker compiles every model with, as
 * TypeScript's own enum values: Monaco's API names no enum for
 * `moduleDetection`.
 *
 * The worker keeps every open model in one program. `moduleDetection: Force`
 * makes each patch and docs example a module of its own, so no model's
 * top-level names reach another's scope; the DSL lib, a declaration file,
 * stays global.
 */
export const EDITOR_COMPILER_OPTIONS = {
    allowJs: true,
    allowNonTsExtensions: true,
    // Top-level labels are track labels, which no statement jumps to.
    allowUnusedLabels: true,
    checkJs: true,
    lib: ['esnext'],
    module: 99 satisfies TS.ModuleKind.ESNext,
    moduleDetection: 3 satisfies TS.ModuleDetectionKind.Force,
    moduleResolution: 2 satisfies TS.ModuleResolutionKind.Node10,
    noEmit: true,
    target: 7 satisfies TS.ScriptTarget.ES2020,
};
