import { readdirSync } from 'node:fs';
import { join, relative } from 'node:path';
import { describe, expect, it } from 'vitest';
import { BUILTIN_PREFIX, BUILTIN_WAVETABLES } from './builtinAssets';

const builtinDir = join(__dirname, '..', '..', 'assets', 'builtin');

/** Every WAV under `dir`, as the `builtin/` path a patch writes. */
function bundledWavs(dir: string): string[] {
    return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
        const full = join(dir, entry.name);
        if (entry.isDirectory()) return bundledWavs(full);
        return entry.name.endsWith('.wav')
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
            bundledWavs(join(builtinDir, 'tables')).sort(),
        );
    });
});
