import { describe, expect, test } from 'vitest';
import {
    normalizeFileName,
    renamedFileName,
    renamedFilePath,
} from '../buffers';

describe('normalizeFileName', () => {
    test('a name without a patch extension is saved as .sl', () => {
        expect(normalizeFileName(' song ')).toBe('song.sl');
        expect(normalizeFileName('song.txt')).toBe('song.txt.sl');
    });

    test('a name with a patch extension is kept', () => {
        expect(normalizeFileName('song.sl')).toBe('song.sl');
        expect(normalizeFileName('song.mjs')).toBe('song.mjs');
        expect(normalizeFileName('song.js')).toBe('song.js');
    });
});

describe('renamedFileName', () => {
    test('a patch keeps its own extension when the new name has none', () => {
        expect(renamedFileName('/w/old.mjs', 'new')).toBe('new.mjs');
        expect(renamedFileName('/w/old.js', 'new')).toBe('new.js');
        expect(renamedFileName('/w/old.sl', 'new')).toBe('new.sl');
        expect(renamedFileName('/w/Old.MJS', 'new')).toBe('new.MJS');
    });

    test('a patch takes a patch extension the new name gives', () => {
        expect(renamedFileName('/w/old.mjs', 'new.sl')).toBe('new.sl');
    });

    test('any other file takes the new name as typed', () => {
        expect(renamedFileName('/w/kick.wav', 'snare')).toBe('snare');
        expect(renamedFileName('/w/kick.wav', ' snare.wav ')).toBe('snare.wav');
    });
});

describe('renamedFilePath', () => {
    test('the file stays in its folder under the new name', () => {
        expect(renamedFilePath('/w/songs/old.mjs', 'new')).toBe(
            '/w/songs/new.mjs',
        );
        expect(renamedFilePath('C:\\w\\old.sl', 'new')).toBe('C:\\w\\new.sl');
    });

    test('an empty or unchanged name renames nothing', () => {
        expect(renamedFilePath('/w/old.sl', '')).toBeUndefined();
        expect(renamedFilePath('/w/old.sl', 'old.sl')).toBeUndefined();
        expect(renamedFilePath('/w/old.sl', 'old')).toBeUndefined();
    });
});
