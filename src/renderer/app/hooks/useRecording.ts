import { useState } from 'react';
import electronAPI from '../../electronAPI';

/** Whether the engine is recording its output, and the controls for it. */
export function useRecording() {
    const [isRecording, setIsRecording] = useState(false);

    const start = async () => {
        await electronAPI.synthesizer.startRecording();
        setIsRecording(true);
    };
    const stop = async () => {
        await electronAPI.synthesizer.stopRecording();
        setIsRecording(false);
    };

    return {
        isRecording,
        /** The engine stopped recording on its own. */
        markStopped: () => setIsRecording(false),
        start,
        stop,
        /** Flips at once, whatever becomes of the engine call. */
        toggle: () => {
            if (isRecording) {
                void electronAPI.synthesizer.stopRecording();
            } else {
                void electronAPI.synthesizer.startRecording();
            }
            setIsRecording(!isRecording);
        },
    };
}
