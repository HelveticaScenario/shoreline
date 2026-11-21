import { useEffect, useState } from 'react';
import electronAPI from '../electronAPI';
import type { OutputDeviceNotice } from '../../shared/ipcTypes';
import './OutputDeviceBanner.css';

/** How long a notice stays up unless it persists. */
const NOTICE_MS = 10_000;

/**
 * Says what happened to the output device. A notice that persists stays up
 * until dismissed or replaced; one that playback stopped also goes when
 * playback starts again.
 */
export function OutputDeviceBanner({ playing }: { playing: boolean }) {
    const [notice, setNotice] = useState<OutputDeviceNotice | null>(null);

    useEffect(() => electronAPI.audio.onOutputNotice(setNotice), []);

    // Checked only when `playing` changes, so a notice that arrives while the
    // stop it reports is still settling survives until playback restarts.
    const [wasPlaying, setWasPlaying] = useState(playing);
    if (playing !== wasPlaying) {
        setWasPlaying(playing);
        if (playing && notice?.stopped) {
            setNotice(null);
        }
    }

    useEffect(() => {
        if (!notice || notice.persists) {
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
            className={`output-device-banner${notice.stopped ? ' output-device-banner--stopped' : ''}`}
            role={notice.persists ? 'alert' : 'status'}
        >
            <span className="output-device-banner__message">
                {notice.message}
            </span>
            <button
                className="output-device-banner__dismiss"
                onClick={() => setNotice(null)}
            >
                Dismiss
            </button>
        </div>
    );
}
