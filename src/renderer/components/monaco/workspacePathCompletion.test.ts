import { describe, expect, test } from 'vitest';
import type { languages } from 'monaco-editor';
import type { FileTreeEntry } from '../../../shared/ipcTypes';
import {
    WORKSPACE_PATH_MIME,
    installWorkspacePathDrop,
    quotePath,
    registerWorkspacePathCompletionProvider,
    stringLiteralAt,
    assetPaths,
} from './workspacePathCompletion';

const TREE: FileTreeEntry[] = [
    { name: 'song.js', path: 'song.js', type: 'file', fileType: 'js' },
    {
        name: 'wavs',
        path: 'wavs',
        type: 'directory',
        children: [
            {
                name: 'kick.wav',
                path: 'wavs/kick.wav',
                type: 'file',
                fileType: 'wav',
            },
            {
                name: "don't.wav",
                path: "wavs/don't.wav",
                type: 'file',
                fileType: 'wav',
            },
            {
                name: 'pads',
                path: 'wavs/pads',
                type: 'directory',
                children: [
                    {
                        name: 'warm.wav',
                        path: 'wavs/pads/warm.wav',
                        type: 'file',
                        fileType: 'wav',
                    },
                ],
            },
        ],
    },
    {
        name: 'photo.png',
        path: 'pictures/photo.png',
        type: 'file',
        fileType: 'image',
    },
    {
        name: 'loop.mp4',
        path: 'clips/loop.mp4',
        type: 'file',
        fileType: 'video',
    },
];

describe('stringLiteralAt', () => {
    test('finds the literal holding a column, with its content columns', () => {
        const line = "$sampler('/wavs/k', 5)";
        expect(stringLiteralAt(line, 12)).toEqual({
            quote: "'",
            contentStart: 11,
            contentEnd: 18,
            closed: true,
        });
    });

    test('the column just inside either quote counts', () => {
        const line = `x("ab")`;
        expect(stringLiteralAt(line, 4)?.contentStart).toBe(4);
        expect(stringLiteralAt(line, 6)?.contentEnd).toBe(6);
        expect(stringLiteralAt(line, 3)).toBeNull();
    });

    test('skips escaped quotes and earlier literals', () => {
        const line = `f('it\\'s', "/a")`;
        expect(stringLiteralAt(line, 14)?.quote).toBe('"');
    });

    test('an unclosed literal runs to the end of the line', () => {
        expect(stringLiteralAt("f('/wa", 7)).toMatchObject({
            contentEnd: 7,
            closed: false,
        });
    });

    test('nothing inside a line comment', () => {
        expect(stringLiteralAt("// '/wavs/kick.wav'", 6)).toBeNull();
    });
});

describe('quotePath', () => {
    test('keeps the preferred quote when the path does not hold it', () => {
        expect(quotePath('/wavs/kick.wav', "'")).toBe("'/wavs/kick.wav'");
        expect(quotePath('/wavs/kick.wav', '"')).toBe('"/wavs/kick.wav"');
    });

    test('switches to the other quote when the path holds only the preferred one', () => {
        expect(quotePath("/wavs/don't.wav", "'")).toBe(`"/wavs/don't.wav"`);
        expect(quotePath('/wavs/say "hi".wav', '"')).toBe(
            `'/wavs/say "hi".wav'`,
        );
    });

    test('uses a template literal when the path holds both plain quotes', () => {
        expect(quotePath(`/wavs/it's "a".wav`, "'")).toBe(
            '`/wavs/it\'s "a".wav`',
        );
        expect(quotePath(`/wavs/it's "a".wav`, '"')).toBe(
            '`/wavs/it\'s "a".wav`',
        );
    });

    test('keeps the preferred quote and escapes it when the path holds all three', () => {
        expect(quotePath('/wavs/it\'s "a" `b`.wav', "'")).toBe(
            "'/wavs/it\\'s \"a\" `b`.wav'",
        );
        expect(quotePath('/wavs/it\'s "a" ${b}.wav', '"')).toBe(
            '"/wavs/it\'s \\"a\\" ${b}.wav"',
        );
    });

    test('in a template literal, a path holding a backtick or ${ takes a plain quote', () => {
        expect(quotePath('/wavs/`b`.wav', '`')).toBe("'/wavs/`b`.wav'");
        expect(quotePath("/wavs/${'b'}.wav", '`')).toBe('"/wavs/${\'b\'}.wav"');
    });

    test('a template literal escapes backticks and ${ when the path holds every quote', () => {
        expect(quotePath('/wavs/\'"`${x}.wav', '`')).toBe(
            '`/wavs/\'"\\`\\${x}.wav`',
        );
    });

    test('a backslash before ${ in a template literal stays a backslash', () => {
        expect(quotePath('/a\'"\\${b}.wav', '`')).toBe('`/a\'"\\\\\\${b}.wav`');
    });

    test('escapes backslashes', () => {
        expect(quotePath('/wavs/a\\b.wav', "'")).toBe("'/wavs/a\\\\b.wav'");
    });

    test('the result evaluates back to the path', () => {
        for (const path of [
            "/a'b.wav",
            '/a"b.wav',
            `/a'b"c.wav`,
            '/a\\b.wav',
            '/a`${b}`.wav',
            '/a\'b"c`d${e}.wav',
            '/a\\${b}.wav',
            '/a\'"\\${b}.wav',
            '/a\'"`\\${b}\\`.wav',
        ]) {
            for (const q of ["'", '"', '`']) {
                // eslint-disable-next-line no-new-func
                expect(new Function(`return ${quotePath(path, q)}`)()).toBe(
                    path,
                );
            }
        }
    });
});

test('assetPaths lists every asset with its kind, rooted at /', () => {
    expect(assetPaths(TREE)).toEqual([
        { path: '/wavs/kick.wav', kind: 'wav' },
        { path: "/wavs/don't.wav", kind: 'wav' },
        { path: '/wavs/pads/warm.wav', kind: 'wav' },
        { path: '/pictures/photo.png', kind: 'image' },
        { path: '/clips/loop.mp4', kind: 'video' },
    ]);
});

function makeProvider(): languages.CompletionItemProvider {
    let captured: languages.CompletionItemProvider | undefined;
    const monaco = {
        languages: {
            CompletionItemKind: { File: 20 },
            registerCompletionItemProvider: (
                _lang: string,
                p: languages.CompletionItemProvider,
            ) => {
                captured = p;
                return { dispose: () => {} };
            },
        },
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
    } as any;
    registerWorkspacePathCompletionProvider(monaco, async () => TREE);
    return captured!;
}

async function complete(line: string, column: number) {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const any = {} as any;
    return (await makeProvider().provideCompletionItems(
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        { getLineContent: () => line } as any,
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        { lineNumber: 1, column } as any,
        any,
        any,
    )) as languages.CompletionList | undefined;
}

describe('workspace path completion', () => {
    test('offers every asset in a literal starting with /, replacing the literal', async () => {
        const list = await complete("$sampler('/wa', 5)", 13);
        expect(list?.suggestions.map((s) => s.insertText)).toEqual([
            "'/wavs/kick.wav'",
            `"/wavs/don't.wav"`,
            "'/wavs/pads/warm.wav'",
            "'/pictures/photo.png'",
            "'/clips/loop.mp4'",
        ]);
        expect(list?.suggestions.map((s) => s.detail)).toEqual([
            'WAV file',
            'WAV file',
            'WAV file',
            'Image',
            'Video',
        ]);
        expect(list?.suggestions.map((s) => s.filterText)).toEqual([
            "'/wavs/kick.wav",
            "'/wavs/don't.wav",
            "'/wavs/pads/warm.wav",
            "'/pictures/photo.png",
            "'/clips/loop.mp4",
        ]);
        expect(list?.suggestions[0].range).toMatchObject({
            startColumn: 10,
            endColumn: 15,
        });
    });

    test('in a double-quoted literal, a path with a double quote switches to single', async () => {
        const tree: FileTreeEntry[] = [
            {
                name: 'a"b.wav',
                path: 'a"b.wav',
                type: 'file',
                fileType: 'wav',
            },
        ];
        let captured: languages.CompletionItemProvider | undefined;
        registerWorkspacePathCompletionProvider(
            {
                languages: {
                    CompletionItemKind: { File: 20 },
                    registerCompletionItemProvider: (
                        _l: string,
                        p: languages.CompletionItemProvider,
                    ) => {
                        captured = p;
                        return { dispose: () => {} };
                    },
                },
                // eslint-disable-next-line @typescript-eslint/no-explicit-any
            } as any,
            async () => tree,
        );
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        const any = {} as any;
        const list = (await captured!.provideCompletionItems(
            // eslint-disable-next-line @typescript-eslint/no-explicit-any
            { getLineContent: () => '$sampler("/")' } as any,
            // eslint-disable-next-line @typescript-eslint/no-explicit-any
            { lineNumber: 1, column: 12 } as any,
            any,
            any,
        )) as languages.CompletionList;
        expect(list.suggestions[0].insertText).toBe(`'/a"b.wav'`);
    });

    test('in an unclosed literal, replaces from the opening quote up to the cursor', async () => {
        const list = await complete("$sampler('/wa", 14);
        expect(list?.suggestions[0].range).toMatchObject({
            startColumn: 10,
            endColumn: 14,
        });
    });

    test('offers nothing in a literal not starting with /', async () => {
        expect(await complete("$sine('c4')", 9)).toBeUndefined();
    });

    test('offers nothing outside a literal', async () => {
        expect(await complete('$sine(/)', 8)).toBeUndefined();
    });
});

describe('dropping a workspace path', () => {
    function dropOnto(line: string, column: number, path = '/wavs/kick.wav') {
        const listeners: Record<string, (e: DragEvent) => void> = {};
        let current = line;
        const editor = {
            getContainerDomNode: () => ({
                addEventListener: (
                    type: string,
                    fn: (e: DragEvent) => void,
                ) => {
                    listeners[type] = fn;
                },
                removeEventListener: () => {},
            }),
            getModel: () => ({ getLineContent: () => current }),
            getTargetAtClientPoint: () => ({
                position: { lineNumber: 1, column },
            }),
            focus: () => {},
            setPosition: () => {},
            executeEdits: (
                _source: string,
                edits: {
                    range: { startColumn: number; endColumn: number };
                    text: string;
                }[],
            ) => {
                const [{ range, text }] = edits;
                current =
                    current.slice(0, range.startColumn - 1) +
                    text +
                    current.slice(range.endColumn - 1);
            },
        };
        installWorkspacePathDrop(
            // eslint-disable-next-line @typescript-eslint/no-explicit-any
            editor as any,
            () => "'",
        );
        const event = {
            dataTransfer: {
                types: [WORKSPACE_PATH_MIME],
                getData: () => path,
            },
            preventDefault: () => {},
            stopPropagation: () => {},
            clientX: 0,
            clientY: 0,
        } as unknown as DragEvent;
        listeners.drop(event);
        return current;
    }

    test('onto a string literal, replaces its contents', () => {
        expect(dropOnto("$sampler('/wavs/old.wav', 5)", 14)).toBe(
            "$sampler('/wavs/kick.wav', 5)",
        );
    });

    test('onto a literal, a path holding its quote switches quotes', () => {
        expect(dropOnto("$sampler('/old.wav', 5)", 13, "/don't.wav")).toBe(
            `$sampler("/don't.wav", 5)`,
        );
    });

    test('onto a double-quoted literal, the preferred quote wins', () => {
        expect(dropOnto(`$sampler("/old.wav", 5)`, 13)).toBe(
            "$sampler('/wavs/kick.wav', 5)",
        );
    });

    test('a path holding both plain quotes drops as a template literal', () => {
        expect(dropOnto(`$sampler("/old.wav", 5)`, 13, `/it's "x".wav`)).toBe(
            '$sampler(`/it\'s "x".wav`, 5)',
        );
    });

    test('elsewhere, inserts a quoted path', () => {
        expect(dropOnto('$sampler(, 5)', 10)).toBe(
            "$sampler('/wavs/kick.wav', 5)",
        );
    });
});
