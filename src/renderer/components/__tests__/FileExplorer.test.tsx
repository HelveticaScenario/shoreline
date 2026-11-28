// @vitest-environment jsdom

/**
 * Invariant: renamingPath is an absolute path (like buffer identity), while
 * workspace file-tree entries carry workspace-relative paths. The tree must
 * resolve entries to absolute paths before comparing, so renaming a file that
 * is not open in any buffer shows the inline rename input on its tree row and
 * commits under the absolute path.
 */

import { act, createElement } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest';

vi.mock('../FileExplorer.css', () => ({}));
vi.mock('../../electronAPI', () => ({
    default: { showContextMenu: vi.fn() },
}));
vi.mock('../../keybindings/contextKeyBootstrap', () => ({
    bindFileExplorerFocus: () => () => {},
}));

import { FileExplorer } from '../FileExplorer';
import electronAPI from '../../electronAPI';
import type { FileTreeEntry } from '../../../shared/ipcTypes';

const WORKSPACE = '/workspace';

const FILE_TREE: FileTreeEntry[] = [
    {
        children: [
            {
                fileType: 'wav',
                name: 'kick.wav',
                path: 'samples/kick.wav',
                type: 'file',
            },
        ],
        name: 'samples',
        path: 'samples',
        type: 'directory',
    },
    { fileType: 'patch', name: 'song.mjs', path: 'song.mjs', type: 'file' },
];

let root: Root | null = null;
let container: HTMLElement | null = null;

function renderExplorer(props: {
    renamingPath: string | null;
    onRenameCommit?: (path: string, newName: string) => void;
}) {
    container = document.createElement('div');
    document.body.appendChild(container);
    root = createRoot(container);
    act(() => {
        root!.render(
            createElement(FileExplorer, {
                activeBufferId: undefined,
                buffers: [],
                fileTree: FILE_TREE,
                formatLabel: () => '',
                onCloseBuffer: () => {},
                onCreateFile: () => {},
                onKeepBuffer: () => {},
                onMoveEntry: () => {},
                onOpenFile: () => {},
                onRefreshTree: () => {},
                onRenameCancel: () => {},
                onRenameCommit: props.onRenameCommit ?? (() => {}),
                onSelectBuffer: () => {},
                renamingPath: props.renamingPath,
                runningBufferId: null,
                workspaceRoot: WORKSPACE,
            }),
        );
    });
}

// jsdom lays nothing out, so the virtualized tree gets a fixed size to render
// its rows into.
beforeEach(() => {
    (
        globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }
    ).IS_REACT_ACT_ENVIRONMENT = true;
    window.localStorage.clear();
    vi.stubGlobal(
        'ResizeObserver',
        class {
            observe() {}
            disconnect() {}
        },
    );
    for (const [prop, value] of [
        ['offsetWidth', 20],
        ['offsetHeight', 20],
        ['clientWidth', 200],
        ['clientHeight', 400],
    ] as const) {
        vi.spyOn(HTMLElement.prototype, prop, 'get').mockReturnValue(value);
    }
});

afterEach(() => {
    act(() => root?.unmount());
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
    root = null;
    container?.remove();
    container = null;
});

describe('workspace-tree rename', () => {
    test('an unopened file whose absolute path is being renamed shows the rename input', () => {
        renderExplorer({ renamingPath: `${WORKSPACE}/song.mjs` });

        const input = container!.querySelector<HTMLInputElement>(
            '.file-tree .rename-input',
        );
        expect(input).not.toBeNull();
        expect(input!.defaultValue).toBe('song.mjs');
    });

    test('no rename input appears while nothing is being renamed', () => {
        renderExplorer({ renamingPath: null });
        expect(container!.querySelector('.rename-input')).toBeNull();
    });

    test('committing the rename passes the file’s absolute path', () => {
        const onRenameCommit = vi.fn();
        renderExplorer({
            onRenameCommit,
            renamingPath: `${WORKSPACE}/song.mjs`,
        });

        const input = container!.querySelector<HTMLInputElement>(
            '.file-tree .rename-input',
        )!;
        input.value = 'renamed.mjs';
        act(() => {
            input.dispatchEvent(
                new KeyboardEvent('keydown', { bubbles: true, key: 'Enter' }),
            );
        });

        expect(onRenameCommit).toHaveBeenCalledWith(
            `${WORKSPACE}/song.mjs`,
            'renamed.mjs',
        );
    });
});

describe('workspace-tree folders', () => {
    const names = () =>
        [...container!.querySelectorAll('.tree-name')].map(
            (el) => el.textContent,
        );
    const folderRow = () =>
        container!
            .querySelector('.tree-folder')!
            .closest<HTMLElement>('[role="treeitem"]')!;

    test('clicking a folder shows its files and remembers it expanded', () => {
        renderExplorer({ renamingPath: null });
        expect(names()).toEqual(['samples', 'song.mjs']);

        act(() => folderRow().click());
        expect(names()).toEqual(['samples', 'kick.wav', 'song.mjs']);
        expect(
            JSON.parse(
                window.localStorage.getItem('modular_expanded_folders')!,
            ),
        ).toEqual(['samples']);
    });

    test('a folder expanded at the last launch opens expanded', () => {
        window.localStorage.setItem(
            'modular_expanded_folders',
            JSON.stringify(['samples']),
        );
        renderExplorer({ renamingPath: null });
        expect(names()).toEqual(['samples', 'kick.wav', 'song.mjs']);
    });
});

describe('workspace-tree context menu', () => {
    test('right-clicking an asset opens the file menu for its path', () => {
        window.localStorage.setItem(
            'modular_expanded_folders',
            JSON.stringify(['samples']),
        );
        renderExplorer({ renamingPath: null });

        const asset = [...container!.querySelectorAll('.tree-entry')].find(
            (el) => el.textContent === 'kick.wav',
        )!;
        act(() => {
            asset.dispatchEvent(
                new MouseEvent('contextmenu', { bubbles: true }),
            );
        });

        expect(electronAPI.showContextMenu).toHaveBeenCalledWith(
            expect.objectContaining({
                isWorkspaceFile: true,
                path: 'samples/kick.wav',
                type: 'file',
            }),
        );
    });
});
