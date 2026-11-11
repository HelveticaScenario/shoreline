import { ASSET_EXTENSIONS, type AssetKind } from '../../shared/assetFiles';

const EXAMPLE_PATHS: Record<AssetKind, string> = {
    wav: '/wavs/kick.wav',
    image: '/pictures/photo.png',
    video: '/clips/loop.mp4',
};

/**
 * Returns `path` as written when it starts with `/` (the workspace folder),
 * stays inside it and ends in an extension of `kind`; otherwise throws an
 * error naming `fn`.
 */
export function assetPath(fn: string, path: unknown, kind: AssetKind): string {
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
        throw new Error(
            `${fn}: a file is a path starting with "/" (the workspace folder), like '${EXAMPLE_PATHS[kind]}', got ${JSON.stringify(path)}`,
        );
    }
    if (path.split('/').includes('..')) {
        throw new Error(
            `${fn}: path must stay inside the workspace folder, got "${path}"`,
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
