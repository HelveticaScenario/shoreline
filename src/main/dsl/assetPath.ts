import { ASSET_EXTENSIONS, type AssetKind } from '../../shared/assetFiles';
import { BUILTIN_PREFIX, BUILTIN_WAVETABLES } from '../../shared/builtinAssets';
import { BUILTIN_DX7_VOICES } from '../../shared/builtinDx7';

/** The bundled files of each kind, by path. */
const BUILTIN_PATHS: Record<AssetKind, ReadonlySet<string>> = {
    wav: new Set(BUILTIN_WAVETABLES.map((asset) => asset.path)),
    image: new Set(),
    video: new Set(),
    syx: new Set(BUILTIN_DX7_VOICES.map((voice) => voice.path)),
};

const EXAMPLE_PATHS: Record<AssetKind, string> = {
    wav: '/wavs/kick.wav',
    image: '/pictures/photo.png',
    video: '/clips/loop.mp4',
    syx: '/dx7/rom1a.syx',
};

/**
 * Returns `path` as written when it names a bundled file of `kind`, or when it
 * starts with `/` (the workspace folder), continues with plain `/`-separated
 * names and ends in an extension of `kind`; otherwise throws an error naming
 * `fn`.
 */
export function assetPath(fn: string, path: unknown, kind: AssetKind): string {
    const builtins = BUILTIN_PATHS[kind];
    if (typeof path === 'string' && path.startsWith(BUILTIN_PREFIX)) {
        if (builtins.has(path)) return path;
        throw new Error(`${fn}: no built-in file ${JSON.stringify(path)}`);
    }
    if (typeof path !== 'string' || !path.startsWith('/')) {
        const scheme =
            typeof path === 'string'
                ? /^([a-z][a-z0-9+.-]*):\/\//i.exec(path)
                : null;
        if (scheme !== null) {
            throw new Error(
                `${fn}: ${scheme[1]}:// addresses cannot be loaded; use a file in the workspace folder`,
            );
        }
        const builtin =
            builtins.size > 0
                ? `, or a built-in file like '${builtins.values().next().value}'`
                : '';
        throw new Error(
            `${fn}: a file is a path starting with "/" (the workspace folder), like '${EXAMPLE_PATHS[kind]}'${builtin}, got ${JSON.stringify(path)}`,
        );
    }
    const segments = path.slice(1).split('/');
    if (
        path.includes('\\') ||
        segments.some((s) => s === '' || s === '.' || s === '..')
    ) {
        throw new Error(
            `${fn}: path must stay inside the workspace folder, with a plain name between each "/", got ${JSON.stringify(path)}`,
        );
    }
    const extensions = ASSET_EXTENSIONS[kind];
    const extension = path.split('.').pop()?.toLowerCase() ?? '';
    if (!extensions.includes(extension)) {
        throw new Error(
            `${fn}: "${path}" must be one of ${extensions.map((e) => `.${e}`).join(', ')}`,
        );
    }
    return path;
}
