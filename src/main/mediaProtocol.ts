import { protocol } from 'electron';
import fs from 'node:fs';
import path from 'node:path';
import { Readable } from 'node:stream';
import { MEDIA_SCHEME, parseMediaUrl } from '../shared/video/mediaUrl';

/**
 * Lets the renderer load the workspace's pictures and recordings. Must run
 * before the app is ready.
 */
export function registerMediaScheme(): void {
    protocol.registerSchemesAsPrivileged([
        {
            privileges: {
                bypassCSP: true,
                corsEnabled: true,
                secure: true,
                standard: true,
                stream: true,
                supportFetchAPI: true,
            },
            scheme: MEDIA_SCHEME,
        },
    ]);
}

/**
 * The file a media request names, or null when there is no workspace, the URL
 * is malformed, or the path would leave the workspace folder.
 */
export function resolveMediaFile(
    workspaceRoot: string | null,
    url: string,
): string | null {
    const relative = parseMediaUrl(url);
    return relative === null || relative === ''
        ? null
        : workspaceFile(workspaceRoot, relative);
}

/** The absolute path of `relative`, or null when there is no workspace or it would leave the workspace folder. */
function workspaceFile(
    workspaceRoot: string | null,
    relative: string,
): string | null {
    if (workspaceRoot === null) return null;
    const root = path.resolve(workspaceRoot);
    const file = path.resolve(root, relative);
    return file.startsWith(root + path.sep) ? file : null;
}

const CONTENT_TYPES: Record<string, string> = {
    avif: 'image/avif',
    bmp: 'image/bmp',
    gif: 'image/gif',
    jpeg: 'image/jpeg',
    jpg: 'image/jpeg',
    m4v: 'video/x-m4v',
    mov: 'video/quicktime',
    mp4: 'video/mp4',
    ogv: 'video/ogg',
    png: 'image/png',
    webm: 'video/webm',
    webp: 'image/webp',
};

/** The media type a file is served as, or undefined when it is not media. */
function contentType(file: string): string | undefined {
    return CONTENT_TYPES[path.extname(file).slice(1).toLowerCase()];
}

/**
 * The real path of the workspace file `file`, or null when it is not a media
 * file inside the workspace folder. Links are followed before the containment
 * check, so a link inside the workspace cannot reach a file outside it.
 */
function servableFile(
    workspaceRoot: string | null,
    file: string | null,
): string | null {
    if (file === null || workspaceRoot === null) return null;
    try {
        const real = fs.realpathSync(file);
        const root = fs.realpathSync(workspaceRoot);
        return real.startsWith(root + path.sep) &&
            contentType(real) !== undefined &&
            fs.statSync(real).isFile()
            ? real
            : null;
    } catch {
        return null;
    }
}

/** Whether `relative` names a media file inside the workspace folder. */
export function mediaFileExists(
    workspaceRoot: string | null,
    relative: string,
): boolean {
    return (
        servableFile(workspaceRoot, workspaceFile(workspaceRoot, relative)) !==
        null
    );
}

/**
 * The bytes a `Range` header asks of a file of `size` bytes, as an inclusive
 * range; `null` when there is no usable header, which is answered with the
 * whole file; `'unsatisfiable'` when the range lies outside the file.
 */
export function parseByteRange(
    header: string | null,
    size: number,
): { start: number; end: number } | 'unsatisfiable' | null {
    const match = header === null ? null : /^bytes=(\d*)-(\d*)$/.exec(header);
    if (match === null || (match[1] === '' && match[2] === '')) return null;
    let start: number;
    let end: number;
    if (match[1] === '') {
        const suffix = Number(match[2]);
        if (suffix === 0) return 'unsatisfiable';
        start = Math.max(0, size - suffix);
        end = size - 1;
    } else {
        start = Number(match[1]);
        end = match[2] === '' ? size - 1 : Math.min(Number(match[2]), size - 1);
    }
    return start >= size || start > end ? 'unsatisfiable' : { start, end };
}

/**
 * The CORS origin media is served to. The renderer's page is another origin
 * (the dev server, or a file in a packaged build), and a shader reads pixels
 * only from media served with CORS. The scheme exists only inside this app's
 * session, so no page beyond the app's own can request it.
 */
const ANY_ORIGIN = '*';

/**
 * Serves workspace files over the media scheme. Byte ranges are answered so a
 * video can seek, which its loop points depend on.
 */
export function handleMediaProtocol(
    getWorkspaceRoot: () => string | null,
): void {
    protocol.handle(MEDIA_SCHEME, async (request) => {
        const root = getWorkspaceRoot();
        const file = servableFile(root, resolveMediaFile(root, request.url));
        if (file === null) return new Response('Not found', { status: 404 });
        const { size } = await fs.promises.stat(file);
        const headers: Record<string, string> = {
            'Accept-Ranges': 'bytes',
            'Access-Control-Allow-Origin': ANY_ORIGIN,
            'Content-Type': contentType(file)!,
        };
        const range = parseByteRange(request.headers.get('Range'), size);
        if (range === 'unsatisfiable') {
            headers['Content-Range'] = `bytes */${size}`;
            return new Response(null, { headers, status: 416 });
        }
        const { start, end } = range ?? { start: 0, end: size - 1 };
        headers['Content-Length'] = String(size === 0 ? 0 : end - start + 1);
        if (range !== null) {
            headers['Content-Range'] = `bytes ${start}-${end}/${size}`;
        }
        const body =
            size === 0
                ? null
                : (Readable.toWeb(
                      fs.createReadStream(file, { end, start }),
                  ) as ReadableStream);
        return new Response(body, {
            headers,
            status: range === null ? 200 : 206,
        });
    });
}
