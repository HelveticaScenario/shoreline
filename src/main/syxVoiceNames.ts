import * as fs from 'fs';
import type { SyxFileVoices } from '../shared/dx7Voice';

/**
 * Lists the voice names of workspace `.syx` files for path completion,
 * reading a file again only when its modification time changes.
 * `file` gives the absolute file of a workspace path, and `load` reads its
 * voices, throwing when it holds none.
 */
export function createSyxVoiceNames(
    file: (path: string) => string | null,
    load: (path: string) => { name: string }[],
): (paths: string[]) => SyxFileVoices[] {
    const cache = new Map<string, { mtimeMs: number; names: string[] }>();
    return (paths) =>
        paths.flatMap((path) => {
            const absolute = file(path);
            if (!absolute) return [];
            try {
                const { mtimeMs } = fs.statSync(absolute);
                let entry = cache.get(absolute);
                if (entry?.mtimeMs !== mtimeMs) {
                    entry = {
                        mtimeMs,
                        names: load(path).map((voice) => voice.name),
                    };
                    cache.set(absolute, entry);
                }
                return [{ path, names: entry.names }];
            } catch {
                cache.delete(absolute);
                return [];
            }
        });
}
