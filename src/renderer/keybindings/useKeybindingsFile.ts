import { useEffect, useRef, useState } from 'react';
import electronAPI from '../electronAPI';

/**
 * The user's keybindings.json. Its path is resolved up front, so a save of
 * it — even from a session-restored buffer — is recognised and counted in
 * `revision`, which {@link useKeymap} follows to reload the keymap live.
 */
export function useKeybindingsFile() {
    const pathRef = useRef<string | null>(null);
    const [revision, setRevision] = useState(0);

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
                setRevision((count) => count + 1);
            }
        },
        /** How many times the file has been saved. */
        revision,
    };
}
