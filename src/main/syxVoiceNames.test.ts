import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import { afterEach, beforeEach, describe, expect, test } from 'vitest';
import { createSyxVoiceNames } from './syxVoiceNames';

describe('createSyxVoiceNames', () => {
    let dir: string;
    let names: string[];
    let fail: boolean;
    let loads: number;
    let list: ReturnType<typeof createSyxVoiceNames>;

    beforeEach(() => {
        dir = fs.mkdtempSync(path.join(os.tmpdir(), 'syx-names-'));
        fs.writeFileSync(path.join(dir, 'a.syx'), 'x');
        names = ['E.PIANO 1'];
        fail = false;
        loads = 0;
        list = createSyxVoiceNames(
            (p) => (p === '/none.syx' ? null : path.join(dir, p)),
            () => {
                loads++;
                if (fail) throw new Error('holds no voices');
                return names.map((name) => ({ name }));
            },
        );
    });

    afterEach(() => fs.rmSync(dir, { recursive: true, force: true }));

    const touch = (seconds: number) =>
        fs.utimesSync(path.join(dir, 'a.syx'), seconds, seconds);

    test('reads a file again only when its modification time changes', () => {
        touch(1000);
        expect(list(['/a.syx'])).toEqual([
            { path: '/a.syx', names: ['E.PIANO 1'] },
        ]);
        names = ['BASS'];
        expect(list(['/a.syx'])[0].names).toEqual(['E.PIANO 1']);
        expect(loads).toBe(1);
        touch(2000);
        expect(list(['/a.syx'])[0].names).toEqual(['BASS']);
        expect(loads).toBe(2);
    });

    test('skips a file that cannot be read, and reads it again next time', () => {
        touch(1000);
        fail = true;
        expect(list(['/a.syx'])).toEqual([]);
        fail = false;
        expect(list(['/a.syx'])[0].names).toEqual(['E.PIANO 1']);
        expect(loads).toBe(2);
    });

    test('skips missing files and paths with no workspace file', () => {
        expect(list(['/missing.syx', '/none.syx'])).toEqual([]);
        expect(loads).toBe(0);
    });
});
