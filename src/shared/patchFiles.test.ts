import { describe, expect, it } from 'vitest';
import { isPatchPath, patchExtension } from './patchFiles';

describe('isPatchPath', () => {
    it('accepts patch extensions, case-insensitively', () => {
        expect(isPatchPath('foo.sl')).toBe(true);
        expect(isPatchPath('foo.js')).toBe(true);
        expect(isPatchPath('foo.mjs')).toBe(true);
        expect(isPatchPath('/abs/Path/Bar.MJS')).toBe(true);
        expect(isPatchPath('/abs/Path/Bar.SL')).toBe(true);
    });

    it('rejects non-patch files', () => {
        expect(isPatchPath('keybindings.json')).toBe(false);
        expect(isPatchPath('sample.wav')).toBe(false);
        expect(isPatchPath('README.md')).toBe(false);
        expect(isPatchPath('mjs')).toBe(false);
        expect(isPatchPath('sl')).toBe(false);
    });
});

describe('patchExtension', () => {
    it('returns the extension as the path spells it', () => {
        expect(patchExtension('/w/song.sl')).toBe('.sl');
        expect(patchExtension('/w/Song.MJS')).toBe('.MJS');
        expect(patchExtension('/w/kick.wav')).toBeUndefined();
    });
});
