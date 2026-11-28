/// <reference types="vite/client" />

/**
 * Global type declarations for Electron IPC API
 */

import type { ElectronAPI } from '../preload/preload';

interface TestAPI {
    getEditorValue: () => string;
    /** The cursor's offset in the editor's text, or null with no model. */
    getCursorOffset: () => number | null;
    setEditorValue: (code: string) => void;
    executePatch: () => Promise<void>;
    getLastPatchResult: () => any;
    getScopeData: () => Promise<any>;
    getVuMeterData: () => Promise<any>;
    getVuOutputs: () => any[];
    getEqPanels: () => {
        key: string;
        lists: import('./eq/eqPanelModel').EqSectionView[][];
        perLane: boolean;
        visible: boolean;
    }[];
    getAudioHealth: () => Promise<any>;
    isEngineRunning: () => boolean;
    newUntitledFile: () => void;
    openEngineHealth: () => void;
    openModuleProfile: () => void;
    setMixerVisible: (visible: boolean) => void;
    toggleVuMute: (key: string, codeOnly?: boolean) => void;
    toggleVuSolo: (key: string, codeOnly?: boolean) => void;
}

declare global {
    interface Window {
        electronAPI: ElectronAPI;
        __TEST_API__?: TestAPI;
    }
}

export {};
