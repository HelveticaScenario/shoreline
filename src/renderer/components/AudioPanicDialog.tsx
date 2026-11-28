import { useState } from 'react';
import electronAPI from '../electronAPI';
import { useAudioPanicked, usePanicLogDir } from '../queries/engine';
import './AudioPanicDialog.css';

export function AudioPanicDialog() {
    const panic = useAudioPanicked();
    const { data: logDir } = usePanicLogDir();
    const [restarting, setRestarting] = useState(false);
    const [restartError, setRestartError] = useState<string | null>(null);

    const handleRestart = async () => {
        setRestarting(true);
        setRestartError(null);
        try {
            await electronAPI.synthesizer.restartAudio();
            await panic.refetch({ throwOnError: true });
        } catch (e) {
            setRestartError(e instanceof Error ? e.message : String(e));
        }
        setRestarting(false);
    };

    if (!panic.data) return null;

    return (
        <div className="audio-panic-overlay">
            <div
                className="audio-panic-panel"
                role="alertdialog"
                aria-modal="true"
                aria-labelledby="audio-panic-title"
            >
                <h2 id="audio-panic-title">Audio engine crashed</h2>
                <p>
                    The audio thread hit an unrecoverable error and is now
                    silent.
                    {logDir && (
                        <>
                            {' '}
                            A panic log was written to{' '}
                            <button
                                type="button"
                                className="audio-panic-log-link"
                                onClick={() => {
                                    void electronAPI.shell.openPath(logDir);
                                }}
                                title={`Open ${logDir} in your file manager`}
                            >
                                <code>{logDir}</code>
                            </button>
                            .
                        </>
                    )}
                </p>
                <p>Restart the audio engine to resume playback.</p>
                {restartError && (
                    <div className="audio-panic-error">
                        Restart failed: {restartError}
                    </div>
                )}
                <div className="audio-panic-actions">
                    <button
                        className="audio-panic-restart-btn"
                        onClick={handleRestart}
                        disabled={restarting}
                        autoFocus
                    >
                        {restarting ? 'Restarting…' : 'Restart Audio'}
                    </button>
                </div>
            </div>
        </div>
    );
}
