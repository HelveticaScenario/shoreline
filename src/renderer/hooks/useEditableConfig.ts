import { useCallback, useEffect, useState } from 'react';
import type { AppConfig } from '../../shared/ipcTypes';
import electronAPI from '../electronAPI';

/**
 * The config file while `active`, kept current with outside edits, and a
 * function that merges a change into it and saves it at once. Only the changed
 * fields are written, so a copy that has not yet caught up with another edit
 * never writes that edit back over. The third value is false until the file
 * has been read, while the config is still empty.
 */
export function useEditableConfig(
    active: boolean,
): [AppConfig, (partial: Partial<AppConfig>) => void, boolean] {
    const [config, setConfig] = useState<AppConfig>({});
    const [loaded, setLoaded] = useState(false);

    useEffect(() => {
        if (!active) return;
        const load = (next: AppConfig) => {
            setConfig(next);
            setLoaded(true);
        };
        electronAPI.config.read().then(load).catch(console.error);
        return electronAPI.config.onChange(load);
    }, [active]);

    const change = useCallback((partial: Partial<AppConfig>) => {
        setConfig((prev) => ({ ...prev, ...partial }));
        electronAPI.config.write(partial).catch(console.error);
    }, []);

    return [config, change, loaded];
}
