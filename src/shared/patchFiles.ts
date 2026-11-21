/** The extension new patches are saved with. */
export const PATCH_EXTENSION = '.sl';

/**
 * Every extension the app opens, runs and stamps as a patch: `.sl`, plus the
 * `.mjs` and `.js` files patches are also kept in.
 */
const PATCH_EXTENSIONS = [PATCH_EXTENSION, '.mjs', '.js'];

/** The save dialog's filter for patch files. */
export const PATCH_FILE_FILTER = {
    extensions: PATCH_EXTENSIONS.map((ext) => ext.slice(1)),
    name: 'Shoreline Patches',
};

/** Whether `filePath` names a patch file. */
export function isPatchPath(filePath: string): boolean {
    const lower = filePath.toLowerCase();
    return PATCH_EXTENSIONS.some((ext) => lower.endsWith(ext));
}
