import { useEffect, useEffectEvent } from 'react';
import type { QueuedTrigger } from '@modular/core';
import electronAPI from '../../electronAPI';
import {
    type CommandMetadata,
    executeCommand,
    registerCommand,
    unregisterCommand,
} from '../../keybindings/commands';
import { performanceOutput } from '../../video/PerformanceOutput';

export interface AppCommandHandlers {
    submit: (trigger: QueuedTrigger) => void;
    stop: () => void;
    newFile: () => void;
    closeActiveBuffer: () => void;
    save: () => void;
    openWorkspace: () => void;
    openKeybindings: () => void;
    showCommandPalette: () => void;
    toggleRecording: () => void;
    openEngineHealth: () => void;
    openModuleProfile: () => void;
}

const patchMenu = (order: number) => ({
    contextMenu: { group: '1_patch', order },
});

const togglePerformanceCode = () => {
    void electronAPI.config.read().then((config) =>
        electronAPI.config.write({
            performanceCodeVisible: !(config.performanceCodeVisible ?? true),
        }),
    );
};

/** Menu items that run a registered command. */
const MENU_COMMANDS = [
    [electronAPI.onMenuNewFile, 'shoreline.newFile'],
    [electronAPI.onMenuSave, 'shoreline.save'],
    [electronAPI.onMenuStop, 'shoreline.stop'],
    [electronAPI.onMenuCancelQueuedUpdate, 'shoreline.cancelQueuedUpdate'],
    [electronAPI.onMenuUpdatePatch, 'shoreline.updatePatch'],
    [electronAPI.onMenuUpdatePatchNextBeat, 'shoreline.updatePatchNextBeat'],
    [electronAPI.onMenuOpenWorkspace, 'shoreline.openWorkspace'],
    [electronAPI.onMenuCloseBuffer, 'shoreline.closeBuffer'],
    [electronAPI.onMenuToggleMixer, 'shoreline.toggleMixer'],
    [
        electronAPI.onMenuTogglePerformanceWindow,
        'shoreline.togglePerformanceWindow',
    ],
    [electronAPI.onMenuOpenSettings, 'shoreline.openSettings'],
] as const;

/**
 * Registers the app's `shoreline.*` commands in the global registry and
 * routes the Electron menu to them. The menu, the command palette, the
 * editor context menu and the keymap all dispatch through `executeCommand`,
 * so each command's behaviour lives in one place.
 */
export function useAppCommands(handlers: AppCommandHandlers): void {
    // Commands register once and always run the latest handlers.
    const run = useEffectEvent(
        <K extends keyof AppCommandHandlers>(
            name: K,
            ...args: Parameters<AppCommandHandlers[K]>
        ) => (handlers[name] as (...a: unknown[]) => void)(...args),
    );

    useEffect(() => {
        const commands: [string, () => void, CommandMetadata][] = [
            [
                'shoreline.updatePatch',
                () => run('submit', 'NextBar'),
                { category: 'Patch', label: 'Update Patch', ...patchMenu(1) },
            ],
            [
                'shoreline.updatePatchNextBeat',
                () => run('submit', 'NextBeat'),
                {
                    category: 'Patch',
                    label: 'Update Patch (Next Beat)',
                    ...patchMenu(2),
                },
            ],
            [
                'shoreline.stop',
                () => run('stop'),
                { category: 'Patch', label: 'Stop', ...patchMenu(3) },
            ],
            [
                'shoreline.cancelQueuedUpdate',
                () => void electronAPI.synthesizer.cancelQueuedUpdate(),
                {
                    category: 'Patch',
                    label: 'Cancel Queued Update',
                    ...patchMenu(4),
                },
            ],
            [
                'shoreline.newFile',
                () => run('newFile'),
                { category: 'File', label: 'New File' },
            ],
            [
                'shoreline.closeBuffer',
                () => run('closeActiveBuffer'),
                { category: 'File', label: 'Close Buffer' },
            ],
            [
                'shoreline.save',
                () => run('save'),
                { category: 'File', label: 'Save' },
            ],
            [
                'shoreline.openWorkspace',
                () => run('openWorkspace'),
                { category: 'File', label: 'Open Workspace…' },
            ],
            [
                'shoreline.showCommandPalette',
                () => run('showCommandPalette'),
                {
                    category: 'View',
                    contextMenu: { group: 'z_commands', order: 1 },
                    label: 'Show Command Palette',
                },
            ],
            [
                'shoreline.openKeybindings',
                () => run('openKeybindings'),
                {
                    category: 'Preferences',
                    label: 'Open Keyboard Shortcuts (JSON)',
                },
            ],
            [
                'shoreline.togglePerformanceWindow',
                () => performanceOutput.toggle(),
                { category: 'View', label: 'Toggle Performance Window' },
            ],
            [
                'shoreline.togglePerformanceCode',
                togglePerformanceCode,
                { category: 'View', label: 'Toggle Performance Code' },
            ],
        ];
        for (const [id, handler, metadata] of commands) {
            registerCommand(id, handler, metadata);
        }
        return () => {
            for (const [id] of commands) {
                unregisterCommand(id);
            }
        };
    }, []);

    // The Syphon output exists on macOS only; elsewhere its command stays
    // out of the palette, as its menu item does. The action lives in the
    // main process, so the handler round-trips through IPC.
    useEffect(() => {
        let active = true;
        void electronAPI.syphon.isSupported().then((supported) => {
            if (active && supported) {
                registerCommand(
                    'shoreline.toggleSyphon',
                    () => void electronAPI.syphon.toggle(),
                    { category: 'View', label: 'Toggle Syphon Output' },
                );
            }
        });
        return () => {
            active = false;
            unregisterCommand('shoreline.toggleSyphon');
        };
    }, []);

    useEffect(() => {
        const cleanups = [
            ...MENU_COMMANDS.map(([subscribe, id]) =>
                subscribe(() => {
                    executeCommand(id);
                }),
            ),
            electronAPI.onMenuToggleRecording(() => run('toggleRecording')),
            electronAPI.onMenuOpenEngineHealth(() => run('openEngineHealth')),
            electronAPI.onMenuOpenModuleProfile(() => run('openModuleProfile')),
        ];
        return () => cleanups.forEach((cleanup) => cleanup());
    }, []);
}
