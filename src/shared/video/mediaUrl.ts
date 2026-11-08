/** Scheme the app serves workspace media over; the host part is a fixed label. */
export const MEDIA_SCHEME = 'shoreline-media';
const MEDIA_HOST = 'workspace';

/** The URL the renderer loads the workspace file `path` from. */
export function mediaUrl(path: string): string {
    const encoded = path.split('/').map(encodeURIComponent).join('/');
    return `${MEDIA_SCHEME}://${MEDIA_HOST}/${encoded}`;
}

/**
 * The workspace-relative path a media URL names, or null when the URL is not a
 * well-formed media URL. The result may still contain `..`; resolving it
 * against the workspace folder and checking containment is the caller's job.
 */
export function parseMediaUrl(url: string): string | null {
    try {
        const parsed = new URL(url);
        if (
            parsed.protocol !== `${MEDIA_SCHEME}:` ||
            parsed.host !== MEDIA_HOST
        ) {
            return null;
        }
        return parsed.pathname
            .slice(1)
            .split('/')
            .map(decodeURIComponent)
            .join('/');
    } catch {
        return null;
    }
}
