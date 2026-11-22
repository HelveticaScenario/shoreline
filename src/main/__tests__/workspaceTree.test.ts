import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import { describe, expect, test } from 'vitest';
import type { FileTreeEntry } from '../../shared/ipcTypes';
import { resolveWorkspacePath } from '../workspacePaths';
import { readWorkspaceTree } from '../workspaceTree';

/** Every file path in a tree. */
function files(entries: FileTreeEntry[]): string[] {
    return entries.flatMap((e) =>
        e.type === 'file' ? [e.path] : files(e.children ?? []),
    );
}

function tempFolder(name: string): string {
    return fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), `${name}-`)));
}

describe('readWorkspaceTree', () => {
    const root = tempFolder('tree-root');
    const library = tempFolder('tree-library');
    fs.writeFileSync(path.join(root, 'song.mjs'), '');
    fs.mkdirSync(path.join(root, 'own'));
    fs.writeFileSync(path.join(root, 'own', 'snare.wav'), '');
    fs.writeFileSync(path.join(library, 'kick.wav'), '');
    fs.mkdirSync(path.join(library, 'loops'));
    fs.writeFileSync(path.join(library, 'loops', 'break.wav'), '');
    fs.symlinkSync(library, path.join(root, 'library'));
    fs.symlinkSync(
        path.join(library, 'kick.wav'),
        path.join(root, 'kick-link.wav'),
    );
    fs.symlinkSync(path.join(root, 'own'), path.join(root, 'own-link'));
    fs.symlinkSync(root, path.join(root, 'own', 'back-to-root'));
    fs.symlinkSync(path.join(root, 'gone'), path.join(root, 'broken.wav'));
    fs.mkdirSync(path.join(root, '.stash'));
    fs.writeFileSync(path.join(root, '.stash', 'hat.wav'), '');
    fs.symlinkSync(path.join(root, '.stash'), path.join(root, 'stash'));

    const tree = readWorkspaceTree(root);

    test('lists files through links to files and folders', () => {
        expect(files(tree.entries).sort()).toEqual([
            'kick-link.wav',
            'library/kick.wav',
            'library/loops/break.wav',
            'own-link/snare.wav',
            'own/snare.wav',
            'song.mjs',
            'stash/hat.wav',
        ]);
    });

    test('shows a linked folder as a folder', () => {
        expect(tree.entries.find((e) => e.name === 'library')?.type).toBe(
            'directory',
        );
    });

    test('maps only the linked folders outside the workspace to their links', () => {
        expect([...tree.linkedFolders]).toEqual([[library, 'library']]);
    });

    test('names the linked folders a watch on the workspace cannot see', () => {
        expect([...tree.unwatchedFolders].sort()).toEqual(
            [library, path.join(root, '.stash')].sort(),
        );
    });

    test('names where each broken link points', () => {
        expect([...tree.missingTargets]).toEqual([path.join(root, 'gone')]);
    });
});

describe('resolveWorkspacePath through links', () => {
    const root = tempFolder('paths-root');
    const library = tempFolder('paths-library');
    fs.symlinkSync(library, path.join(root, 'library'));
    const links = new Map([[library, 'library']]);

    test('names a file in a linked folder through its link', () => {
        expect(
            resolveWorkspacePath(root, path.join(library, 'new.mjs'), links),
        ).toBe(path.join(root, 'library', 'new.mjs'));
    });

    test('still rejects a folder no link leads to', () => {
        expect(
            resolveWorkspacePath(root, path.join(os.tmpdir(), 'x.mjs'), links),
        ).toBeNull();
    });

    test('still rejects a relative path that climbs out', () => {
        expect(
            resolveWorkspacePath(
                root,
                path.join('..', path.basename(library), 'x.mjs'),
                links,
            ),
        ).toBeNull();
    });
});
