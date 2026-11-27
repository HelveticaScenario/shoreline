import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, test } from 'vitest';
import { parseSyx, validatePatchGraph, type ModuleSchema } from '@modular/core';
import schemas from '@modular/core/schemas.json';
import { executePatchScript } from '../executor';
import { splitVoice, voiceFragments } from '../../../shared/dx7Voice';

const ASSETS = join(__dirname, '../../../../assets');

/** A workspace with one three-voice file, `/dx7/rom.syx`, two voices sharing a name. */
function loadSyx(path: string) {
    if (path.startsWith('builtin/')) {
        return parseSyx(readFileSync(join(ASSETS, path)));
    }
    if (path !== '/dx7/rom.syx') {
        throw new Error(`no file ${path} in the workspace folder`);
    }
    const voice = (name: string, byte: string) => ({
        name,
        data: byte.repeat(128),
    });
    return [
        voice('E.PIANO 1', '01'),
        voice('BASS', '02'),
        voice('E.PIANO 1', '03'),
    ];
}

function voiceOf(source: string) {
    const { patch } = executePatchScript(source, schemas as ModuleSchema[], {
        sampleRate: 48_000,
        workspaceRoot: '/workspace',
        loadSyx,
    });
    const sixOp = patch.modules.find((m) => m.moduleType === '$mi.sixOp');
    return { patch, voice: sixOp?.params.voice };
}

describe('$mi.sixOp voices', () => {
    test('a #name picks that voice of the file', () => {
        expect(
            voiceOf("out: $mi.sixOp('c3', '/dx7/rom.syx#BASS')").voice,
        ).toEqual({
            type: 'dx7_voice',
            path: '/dx7/rom.syx',
            name: 'BASS',
            data: '02'.repeat(128),
        });
    });

    test('a file alone plays its first voice', () => {
        expect(voiceOf("out: $mi.sixOp('c3', '/dx7/rom.syx')").voice.name).toBe(
            'E.PIANO 1',
        );
    });

    test('@index picks a voice whose name an earlier voice has', () => {
        const { voice } = voiceOf("out: $mi.sixOp('c3', '/dx7/rom.syx#@2')");
        expect(voice.name).toBe('E.PIANO 1');
        expect(voice.data).toBe('03'.repeat(128));
    });

    test('an unknown name lists the voices', () => {
        expect(() =>
            voiceOf("out: $mi.sixOp('c3', '/dx7/rom.syx#PAD')"),
        ).toThrow(
            /\$mi\.sixOp: \/dx7\/rom\.syx has no voice "PAD"; its voices are "E\.PIANO 1", "BASS", "@2"/,
        );
    });

    test('a path that is not a .syx file is refused', () => {
        expect(() => voiceOf("out: $mi.sixOp('c3', '/dx7/rom.wav')")).toThrow(
            /must be one of \.syx/,
        );
    });

    test('a builtin voice makes a patch the engine accepts', () => {
        const { patch, voice } = voiceOf(
            "out: $mi.sixOp('c3', 'builtin/dx7/plaits/e-piano-1.syx')",
        );
        expect(voice.name).toBe('E.PIANO 1');
        expect(validatePatchGraph(patch)).toEqual([]);
    });
});

describe('voice strings', () => {
    test('split at the first # after .syx', () => {
        expect(splitVoice('/a#b.syx#C#D')).toEqual({
            path: '/a#b.syx',
            voice: 'C#D',
        });
        expect(splitVoice('/a.SYX#x')).toEqual({ path: '/a.SYX', voice: 'x' });
        expect(splitVoice('/a.syx')).toEqual({ path: '/a.syx', voice: null });
        // Lowercasing İ lengthens the string; the split still lands right.
        expect(splitVoice('/İ.syx#BASS')).toEqual({
            path: '/İ.syx',
            voice: 'BASS',
        });
    });

    test('a repeated name, or one that reads as an index, is named by index', () => {
        expect(voiceFragments(['A', 'B', 'A', '@0'])).toEqual([
            'A',
            'B',
            '@2',
            '@3',
        ]);
    });
});
