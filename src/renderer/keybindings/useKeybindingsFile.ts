import { useEffect, useRef } from 'react';
import electronAPI from '../electronAPI';

/** Tells the keymap its file changed (see {@link useKeymap}). */
export const KEYBINDINGS_CHANGED_EVENT = 'shoreline:keybindings-changed';

/**
 * The user's keybindings.json. Its path is resolved up front, so a save of
 * it — even from a session-restored buffer — is recognised and reloads the
 * keymap live.
 */
export function useKeybindingsFile() {
    const pathRef = useRef<string | null>(null);

    useEffect(() => {
        electronAPI.keybindings
            .getPath()
            .then((path) => {
                pathRef.current = path;
            })
            .catch(() => {});
    }, []);

    return {
        /** The file's path, after creating the file from a template if it
         *  does not exist. */
        ensureFile: async () => {
            const path = await electronAPI.keybindings.ensureFile();
            pathRef.current = path;
            return path;
        },
        /** Call with every saved file's absolute path. */
        onFileSaved: (filePath: string) => {
            if (filePath === pathRef.current) {
                window.dispatchEvent(new Event(KEYBINDINGS_CHANGED_EVENT));
            }
        },
    };
}
