import { useEffect, useRef } from 'react';
import { getBufferId } from '../app/buffers';
import type { EditorBuffer } from '../types/editor';
import type { MixerController } from './MixerController';

/**
 * Tells the mixer which buffer the editor shows, and returns its jump
 * gesture: move the cursor to a track's label, first switching the editor to
 * the buffer the mixer's tracks came from.
 */
export function useMixerJump({
    mixer,
    buffers,
    visibleBufferId,
    showBuffer,
    jumpTo,
}: {
    mixer: MixerController;
    buffers: EditorBuffer[];
    /** Stable id of the buffer the editor shows. */
    visibleBufferId: string | undefined;
    /** Show the buffer with this path identity in the editor. */
    showBuffer: (bufferId: string) => void;
    /** Move the cursor to an offset in the visible buffer. */
    jumpTo: (offset: number) => void;
}): (key: string) => void {
    /** A jump waiting for the editor to show the mixer's buffer. */
    const pendingRef = useRef<string | null>(null);

    // A pending jump lands once the mixer knows the editor shows its buffer;
    // by then the editor has swapped in the buffer's model, which the label
    // anchors were placed on.
    useEffect(() => {
        mixer.edits.setVisibleBuffer(visibleBufferId);
        const key = pendingRef.current;
        const offset = key === null ? null : mixer.edits.labelOffset(key);
        if (offset !== null) {
            pendingRef.current = null;
            jumpTo(offset);
        }
    }, [visibleBufferId, mixer, jumpTo]);

    return (key: string) => {
        const offset = mixer.edits.labelOffset(key);
        if (offset !== null) {
            jumpTo(offset);
            return;
        }
        const source = buffers.find((b) => b.id === mixer.edits.sourceId);
        if (source) {
            pendingRef.current = key;
            showBuffer(getBufferId(source));
        }
    };
}
