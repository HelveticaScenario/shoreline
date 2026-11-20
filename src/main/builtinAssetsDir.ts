import { app } from 'electron';
import * as path from 'path';

/** The folder holding the assets bundled with the app, which `builtin/` paths name. */
export function builtinAssetsDir(): string {
    if (app.isPackaged) {
        return path.join(process.resourcesPath, 'builtin');
    }
    // __dirname is <root>/.vite/build in the forge+vite dev build.
    return path.join(__dirname, '..', '..', 'assets', 'builtin');
}
