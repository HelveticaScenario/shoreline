import { useEffect, useState } from 'react';
import electronAPI from '../electronAPI';
import type { DeviceSwitchNotice } from '../../shared/ipcTypes';
import './DeviceSwitchBanner.css';

/** How long a notice stays up when it stopped no playback. */
const NOTICE_MS = 10_000;

/**
 * Says that the audio moved to a new system default output. A notice that
 * playback stopped stays up until dismissed.
 */
export function DeviceSwitchBanner() {
    const [notice, setNotice] = useState<DeviceSwitchNotice | null>(null);

    useEffect(() => electronAPI.audio.onOutputSwitched(setNotice), []);

    useEffect(() => {
        if (!notice || notice.stopped) {
            return;
        }
        const timer = setTimeout(() => setNotice(null), NOTICE_MS);
        return () => clearTimeout(timer);
    }, [notice]);

    if (!notice) {
        return null;
    }
    return (
        <div
            className={`device-switch-banner${notice.stopped ? ' device-switch-banner--stopped' : ''}`}
            role={notice.stopped ? 'alert' : 'status'}
        >
            <span className="device-switch-banner__message">
                {notice.message}
            </span>
            <button
                className="device-switch-banner__dismiss"
                onClick={() => setNotice(null)}
            >
                Dismiss
            </button>
        </div>
    );
}
