import { describe, expect, test } from 'vitest';
import { DEFAULT_KEYMAP } from '../../keybindings/defaultKeymap';
import { mergeKeymap } from '../../keybindings/keymap';
import { GUIDE_CHAPTERS, chapterSections, sectionId } from './chapterList';

const sources = import.meta.glob<string>('./*.mdx', {
    eager: true,
    import: 'default',
    query: '?raw',
});

const files = Object.keys(sources).map((path) =>
    path.replace(/^\.\/|\.mdx$/g, ''),
);

describe('the guide', () => {
    test('lists every chapter file once, and only those', () => {
        expect(GUIDE_CHAPTERS.map((c) => c.id).sort()).toEqual(files.sort());
    });

    test.each(files)('%s gives every section a distinct anchor', (file) => {
        const ids = chapterSections(sources[`./${file}.mdx`]).map(sectionId);
        expect(ids.every((id) => id.length > 0)).toBe(true);
        expect(new Set(ids).size).toBe(ids.length);
    });

    const commands = Object.values(sources).flatMap((source) =>
        [...source.matchAll(/<Key command="([^"]+)" \/>/g)].map((m) => m[1]),
    );

    test.each(['darwin', 'other'] as const)(
        'every shortcut it names has a default binding on %s',
        (platform) => {
            expect(commands.length).toBeGreaterThan(5);
            const bound = new Set(
                mergeKeymap(DEFAULT_KEYMAP, [], platform).map((e) => e.command),
            );
            expect(commands.filter((c) => !bound.has(c))).toEqual([]);
        },
    );
});
