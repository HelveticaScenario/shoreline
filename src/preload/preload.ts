// See the Electron documentation for details on how to use preload scripts:
// https://www.electronjs.org/docs/latest/tutorial/process-model#preload-scripts
import { contextBridge, ipcRenderer } from 'electron/renderer';
import {
    AppConfig,
    ContextMenuAction,
    IPCHandlers,
    IPC_CHANNELS,
    MENU_CHANNELS,
    MainLogEntry,
    Promisify,
    UpdateAvailableInfo,
} from '../shared/ipcTypes';
import type {
    AudioDeviceNotice,
    FileChange,
    QueuedTrigger,
} from '../shared/ipcTypes';
import type {
    VideoShaderUpdate,
    VideoUniformUpdate,
} from '../shared/video/videoGraph';

type ChannelKey = keyof typeof IPC_CHANNELS;

/** The renderer-side signature of the invoke channel `K`. */
type Invoke<K extends ChannelKey> = Promisify<
    IPCHandlers[(typeof IPC_CHANNELS)[K]]
>;

type InvokeTable = Record<string, ChannelKey | Record<string, ChannelKey>>;

/** Each table entry as the method that invokes its channel. */
type InvokeAPI<T extends InvokeTable> = {
    [N in keyof T]: T[N] extends ChannelKey
        ? Invoke<T[N]>
        : {
              [M in keyof T[N]]: T[N][M] extends ChannelKey
                  ? Invoke<T[N][M]>
                  : never;
          };
};

/**
 * Every renderer method that invokes a main-process handler, by the
 * `IPC_CHANNELS` key it forwards to; `IPCHandlers` gives each its signature.
 */
const INVOKE_METHODS = {
    getSchemas: 'GET_SCHEMAS',
    executeDSL: 'DSL_EXECUTE',
    getDslLibSource: 'GET_DSL_LIB_SOURCE',
    getSignalSiteTables: 'GET_SIGNAL_SITE_TABLES',
    synthesizer: {
        getSampleRate: 'SYNTH_GET_SAMPLE_RATE',
        getChannels: 'SYNTH_GET_CHANNELS',
        getScopes: 'SYNTH_GET_SCOPES',
        getVuMeters: 'SYNTH_GET_VU_METERS',
        getModuleStates: 'SYNTH_GET_MODULE_STATES',
        updatePatch: 'SYNTH_UPDATE_PATCH',
        startRecording: 'SYNTH_START_RECORDING',
        stopRecording: 'SYNTH_STOP_RECORDING',
        isRecording: 'SYNTH_IS_RECORDING',
        getHealth: 'SYNTH_GET_HEALTH',
        getModuleProfile: 'SYNTH_GET_MODULE_PROFILE',
        setModuleProfilingEnabled: 'SYNTH_SET_MODULE_PROFILING_ENABLED',
        setModuleProfilingSampleRate: 'SYNTH_SET_MODULE_PROFILING_SAMPLE_RATE',
        stop: 'SYNTH_STOP',
        cancelQueuedUpdate: 'SYNTH_CANCEL_QUEUED_UPDATE',
        isStopped: 'SYNTH_IS_STOPPED',
        setModuleParam: 'SYNTH_SET_MODULE_PARAM',
        getTransportState: 'SYNTH_GET_TRANSPORT_STATE',
        enableLink: 'SYNTH_ENABLE_LINK',
        isAudioThreadPanicked: 'SYNTH_IS_AUDIO_THREAD_PANICKED',
        restartAudio: 'SYNTH_RESTART_AUDIO',
        panicLogDir: 'SYNTH_PANIC_LOG_DIR',
    },
    audio: {
        refreshDeviceCache: 'AUDIO_REFRESH_DEVICE_CACHE',
        getDeviceCache: 'AUDIO_GET_DEVICE_CACHE',
        getCurrentState: 'AUDIO_GET_CURRENT_STATE',
        recreateStreams: 'AUDIO_RECREATE_STREAMS',
        getInputChannels: 'AUDIO_GET_INPUT_CHANNELS',
    },
    midi: {
        listInputs: 'MIDI_LIST_INPUTS',
        getInput: 'MIDI_GET_INPUT',
        setInput: 'MIDI_SET_INPUT',
        tryReconnect: 'MIDI_TRY_RECONNECT',
    },
    filesystem: {
        selectWorkspace: 'FS_SELECT_WORKSPACE',
        getWorkspace: 'FS_GET_WORKSPACE',
        listFiles: 'FS_LIST_FILES',
        listSyxVoices: 'FS_LIST_SYX_VOICES',
        readFile: 'FS_READ_FILE',
        writeFile: 'FS_WRITE_FILE',
        renameFile: 'FS_RENAME_FILE',
        deleteFile: 'FS_DELETE_FILE',
        moveFile: 'FS_MOVE_FILE',
        createFolder: 'FS_CREATE_FOLDER',
        showSaveDialog: 'FS_SHOW_SAVE_DIALOG',
        showInputDialog: 'FS_SHOW_INPUT_DIALOG',
        watchOpenFiles: 'FS_WATCH_OPEN_FILES',
    },
    showContextMenu: 'SHOW_CONTEXT_MENU',
    showUnsavedChangesDialog: 'SHOW_UNSAVED_CHANGES_DIALOG',
    performanceWindow: {
        /** Tells the main process whether the performance window is open, so it keeps this window painting. */
        setOpen: 'VIDEO_SET_OUTPUT_OPEN',
    },
    video: {
        getShader: 'VIDEO_GET_SHADER',
        /** Reads the taps that `restart` names from the engine's newest samples, the rest from where the last pull left off. */
        pull: 'VIDEO_PULL',
        /** Asks the system for camera access when it has not been asked; whether it is allowed. */
        requestCameraAccess: 'VIDEO_REQUEST_CAMERA',
        /** The capture source of display number `display`, counting from 1, or why there is none. */
        getScreenSource: 'VIDEO_SCREEN_SOURCE',
        sendCvValues: 'VIDEO_CV_VALUES',
    },
    config: {
        getPath: 'CONFIG_GET_PATH',
        read: 'CONFIG_READ',
        write: 'CONFIG_WRITE',
    },
    clipboard: {
        writeText: 'CLIPBOARD_WRITE_TEXT',
    },
    /** Overrides read from `<userData>/keybindings.json`. */
    keybindings: {
        getPath: 'KEYBINDINGS_GET_PATH',
        readUser: 'KEYBINDINGS_READ_USER',
        ensureFile: 'KEYBINDINGS_ENSURE_FILE',
    },
    /** Syphon window output (macOS); the helper lives in the main process. */
    syphon: {
        /** Whether Syphon output is available (macOS 14+). */
        isSupported: 'SYPHON_IS_SUPPORTED',
        /** Toggle publishing; resolves with the result (main shows any dialog). */
        toggle: 'SYPHON_TOGGLE',
    },
    shell: {
        openPath: 'SHELL_OPEN_PATH',
    },
    update: {
        check: 'UPDATE_CHECK',
        download: 'UPDATE_DOWNLOAD',
        install: 'UPDATE_INSTALL',
    },
    releaseNotes: {
        whatsNew: 'RELEASE_NOTES_WHATS_NEW',
        getAll: 'RELEASE_NOTES_GET_ALL',
    },
} as const satisfies InvokeTable;

/** Subscriptions to channels the main process sends on, and calls that resolve to nothing. */
interface PushAPI {
    platform: NodeJS.Platform;
    audio: {
        onFallbackWarning: (callback: (warning: string) => void) => () => void;
        onDeviceNotice: (
            callback: (notice: AudioDeviceNotice) => void,
        ) => () => void;
    };
    filesystem: {
        /** Calls `callback` when files in the workspace folder change. */
        onTreeChange: (callback: () => void) => () => void;
        onFileChanged: (callback: (change: FileChange) => void) => () => void;
    };
    onMenuNewFile: (callback: () => void) => () => void;
    onMenuSave: (callback: () => void) => () => void;
    onMenuStop: (callback: () => void) => () => void;
    onMenuTogglePerformanceWindow: (callback: () => void) => () => void;
    onMenuCancelQueuedUpdate: (callback: () => void) => () => void;
    onMenuUpdatePatch: (
        callback: (trigger?: QueuedTrigger) => void,
    ) => () => void;
    onMenuUpdatePatchNextBeat: (callback: () => void) => () => void;
    onMenuOpenWorkspace: (callback: () => void) => () => void;
    onMenuCloseBuffer: (callback: () => void) => () => void;
    onMenuToggleRecording: (callback: () => void) => () => void;
    onMenuToggleMixer: (callback: () => void) => () => void;
    onMenuOpenSettings: (callback: () => void) => () => void;
    onMenuOpenEngineHealth: (callback: () => void) => () => void;
    onMenuOpenModuleProfile: (callback: () => void) => () => void;
    onContextMenuCommand: (
        callback: (action: ContextMenuAction) => void,
    ) => () => void;
    video: {
        onShader: (callback: (update: VideoShaderUpdate) => void) => () => void;
        onUniform: (
            callback: (updates: VideoUniformUpdate[]) => void,
        ) => () => void;
    };
    config: {
        onChange: (callback: (config: AppConfig) => void) => () => void;
    };
    menu: {
        /** Push command -> accelerator so the app menu shows live shortcuts. */
        setAccelerators: (accelerators: Record<string, string>) => void;
    };
    onMainLog: (callback: (entry: MainLogEntry) => void) => () => void;
    update: {
        onAvailable: (
            callback: (info: UpdateAvailableInfo) => void,
        ) => () => void;
        onDownloading: (callback: () => void) => () => void;
        onPreparing: (callback: () => void) => () => void;
        onDownloaded: (callback: () => void) => () => void;
        onError: (callback: (message: string) => void) => () => void;
    };
    releaseNotes: {
        onMenuShow: (callback: () => void) => () => void;
    };
}

/** The API exposed to the renderer process as `window.electronAPI`. */
export type ElectronAPI = InvokeAPI<typeof INVOKE_METHODS> & PushAPI;

/**
 * `ipcRenderer.invoke` is untyped; main registers each channel's handler
 * against `IPCHandlers`, which gives the forwarded call its type.
 */
function invoker<K extends ChannelKey>(key: K): Invoke<K> {
    const channel = IPC_CHANNELS[key];
    return ((...args: unknown[]) =>
        ipcRenderer.invoke(channel, ...args)) as Invoke<K>;
}

function buildInvokeAPI<T extends InvokeTable>(table: T): InvokeAPI<T> {
    return Object.fromEntries(
        Object.entries(table).map(([name, entry]) => [
            name,
            typeof entry === 'string'
                ? invoker(entry)
                : Object.fromEntries(
                      Object.entries(entry).map(([method, key]) => [
                          method,
                          invoker(key),
                      ]),
                  ),
        ]),
    ) as InvokeAPI<T>;
}

const pushAPI: PushAPI = {
    platform: process.platform,
    audio: {
        onFallbackWarning: eventHandler(IPC_CHANNELS.AUDIO_FALLBACK_WARNING),
        onDeviceNotice: eventHandler(IPC_CHANNELS.AUDIO_DEVICE_NOTICE),
    },
    filesystem: {
        onTreeChange: eventHandler(IPC_CHANNELS.FS_TREE_ON_CHANGE),
        onFileChanged: eventHandler(IPC_CHANNELS.FS_ON_FILE_CHANGED),
    },
    onMenuNewFile: eventHandler(MENU_CHANNELS.NEW_FILE),
    onMenuSave: eventHandler(MENU_CHANNELS.SAVE),
    onMenuStop: eventHandler(MENU_CHANNELS.STOP),
    onMenuCancelQueuedUpdate: eventHandler(MENU_CHANNELS.CANCEL_QUEUED_UPDATE),
    onMenuUpdatePatch: eventHandler(MENU_CHANNELS.UPDATE_PATCH),
    onMenuUpdatePatchNextBeat: eventHandler(
        MENU_CHANNELS.UPDATE_PATCH_NEXT_BEAT,
    ),
    onMenuOpenWorkspace: eventHandler(MENU_CHANNELS.OPEN_WORKSPACE),
    onMenuCloseBuffer: eventHandler(MENU_CHANNELS.CLOSE_BUFFER),
    onMenuToggleRecording: eventHandler(MENU_CHANNELS.TOGGLE_RECORDING),
    onMenuToggleMixer: eventHandler(MENU_CHANNELS.TOGGLE_MIXER),
    onMenuTogglePerformanceWindow: eventHandler(
        MENU_CHANNELS.TOGGLE_PERFORMANCE_WINDOW,
    ),
    onMenuOpenSettings: eventHandler(MENU_CHANNELS.OPEN_SETTINGS),
    onMenuOpenEngineHealth: eventHandler(MENU_CHANNELS.OPEN_ENGINE_HEALTH),
    onMenuOpenModuleProfile: eventHandler(MENU_CHANNELS.OPEN_MODULE_PROFILE),
    onContextMenuCommand: eventHandler(IPC_CHANNELS.ON_CONTEXT_MENU_COMMAND),
    video: {
        onShader: eventHandler<[VideoShaderUpdate]>(
            IPC_CHANNELS.VIDEO_ON_SHADER,
        ),
        onUniform: eventHandler<[VideoUniformUpdate[]]>(
            IPC_CHANNELS.VIDEO_ON_UNIFORM,
        ),
    },
    config: {
        onChange: eventHandler(IPC_CHANNELS.CONFIG_ON_CHANGE),
    },
    menu: {
        setAccelerators: (accelerators) => {
            void ipcRenderer.invoke(
                IPC_CHANNELS.MENU_SET_ACCELERATORS,
                accelerators,
            );
        },
    },
    onMainLog: eventHandler(IPC_CHANNELS.MAIN_LOG),
    update: {
        onAvailable: eventHandler<[UpdateAvailableInfo]>(
            IPC_CHANNELS.UPDATE_AVAILABLE,
        ),
        onDownloaded: eventHandler(IPC_CHANNELS.UPDATE_DOWNLOADED),
        onDownloading: eventHandler(IPC_CHANNELS.UPDATE_DOWNLOADING),
        onPreparing: eventHandler(IPC_CHANNELS.UPDATE_PREPARING),
        onError: eventHandler<[string]>(IPC_CHANNELS.UPDATE_ERROR),
    },
    releaseNotes: {
        onMenuShow: eventHandler(MENU_CHANNELS.SHOW_RELEASE_NOTES),
    },
};

/** Joins the two halves, merging the namespaces both have. */
function mergeAPIs<A extends object, B extends object>(a: A, b: B): A & B {
    const merged = { ...a } as Record<string, unknown>;
    for (const [name, value] of Object.entries(b)) {
        const existing = merged[name];
        merged[name] =
            typeof existing === 'object' && typeof value === 'object'
                ? { ...existing, ...value }
                : value;
    }
    return merged as A & B;
}

const electronAPI: ElectronAPI = mergeAPIs(
    buildInvokeAPI(INVOKE_METHODS),
    pushAPI,
);

contextBridge.exposeInMainWorld('electronAPI', electronAPI);

function eventHandler<T extends any[]>(
    channel: string,
): (callback: (...args: T) => void) => () => Electron.IpcRenderer {
    return (callback: (...args: T) => void) => {
        const subscription = (_event: any, ...args: T) => callback(...args);
        ipcRenderer.on(channel, subscription);
        return () => ipcRenderer.removeListener(channel, subscription);
    };
}
