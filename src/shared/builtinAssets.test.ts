import { readdirSync } from 'node:fs';
import { join, relative } from 'node:path';
import { describe, expect, it } from 'vitest';
import { BUILTIN_PREFIX, BUILTIN_WAVETABLES } from './builtinAssets';
import { BUILTIN_DX7_VOICES } from './builtinDx7';

const builtinDir = join(__dirname, '..', '..', 'assets', 'builtin');

/** Every file under `dir` ending in `extension`, as the `builtin/` path a patch writes. */
function bundled(dir: string, extension: string): string[] {
    return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
        const full = join(dir, entry.name);
        if (entry.isDirectory()) return bundled(full, extension);
        return entry.name.endsWith(extension)
            ? [
                  BUILTIN_PREFIX +
                      relative(builtinDir, full).split('\\').join('/'),
              ]
            : [];
    });
}

describe('built-in assets', () => {
    it('lists exactly the bundled wavetables', () => {
        expect(BUILTIN_WAVETABLES.map((a) => a.path).sort()).toEqual(
            bundled(join(builtinDir, 'tables'), '.wav').sort(),
        );
    });

    it('lists exactly the bundled DX7 voices', () => {
        expect(BUILTIN_DX7_VOICES.map((v) => v.path).sort()).toEqual(
            bundled(join(builtinDir, 'dx7'), '.syx').sort(),
        );
    });
});
