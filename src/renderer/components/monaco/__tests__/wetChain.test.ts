import { describe, expect, test } from 'vitest';

import { formatDsl } from '../formattingProvider';

describe('wetChain', () => {
    test('keeps .wet on the line of the call it mixes against', async () => {
        expect(
            await formatDsl(
                `out: $saw('c3').lpf('800hz').delay('1/8', {feedback: 3}).wet(2).plate().wet(1)`,
            ),
        ).toBe(
            [
                `out: $saw('c3')`,
                `  .lpf('800hz')`,
                `  .delay('1/8', { feedback: 3 }).wet(2)`,
                `  .plate().wet(1)`,
            ].join('\n'),
        );
    });

    test('keeps .wet on the head of a broken chain', async () => {
        expect(
            await formatDsl(
                `out: $saw('c3', {shape: 2}).wet(2).lpf('800hz', {res: 3, drive: 2}).plate()`,
            ),
        ).toBe(
            [
                `out: $saw('c3', { shape: 2 }).wet(2)`,
                `  .lpf('800hz', { res: 3, drive: 2 })`,
                `  .plate()`,
            ].join('\n'),
        );
    });

    test('leaves chains that fit on one line alone', async () => {
        expect(await formatDsl(`out: $saw('c3').plate().wet(1.5)`)).toBe(
            `out: $saw('c3').plate().wet(1.5)`,
        );
    });

    test('formats its own output unchanged', async () => {
        const once = await formatDsl(
            `out: $saw('c3').lpf('800hz').delay('1/8', {feedback: 3}).wet(2).plate().wet(1)`,
        );
        expect(await formatDsl(once)).toBe(once);
    });
});
