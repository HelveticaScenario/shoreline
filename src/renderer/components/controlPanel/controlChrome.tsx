/**
 * Pieces shared by the Controls panel and the mixer: the Ctrl/Cmd "go to
 * code" gesture, the out-of-sync marker, and the tooltip that explains both.
 */

import { useCallback, useEffect, useState } from 'react';
import type { ControlBinding } from '../../app/controlBinding';

const JUMP_HINT = '⌃/⌘-click: go to code';

/** Ctrl or Cmd held: the jump gesture, and in the mixer the code-only one. */
export function isJumpGesture(e: {
    ctrlKey: boolean;
    metaKey: boolean;
}): boolean {
    return e.ctrlKey || e.metaKey;
}

/**
 * A Ctrl/Cmd-click on a jump target moves the editor cursor to its place in
 * the source instead of operating it. The handlers run in the capture phase so
 * the control never sees the gesture; on macOS Ctrl+click also raises a
 * context menu, which is suppressed.
 */
export function jumpHandlers<T>(target: T, onJump: (target: T) => void) {
    return {
        onClickCapture: (e: React.MouseEvent) => {
            if (isJumpGesture(e)) {
                e.preventDefault();
                e.stopPropagation();
            }
        },
        onContextMenu: (e: React.MouseEvent) => {
            if (e.ctrlKey) {
                e.preventDefault();
            }
        },
        onMouseDownCapture: (e: React.MouseEvent) => {
            if (isJumpGesture(e)) {
                e.preventDefault();
                e.stopPropagation();
            }
        },
        onPointerDownCapture: (e: React.PointerEvent) => {
            if (isJumpGesture(e)) {
                e.preventDefault();
                e.stopPropagation();
                onJump(target);
            }
        },
    };
}

/**
 * Whether Ctrl/Cmd is held, so the hovered target can show it is jumpable.
 * Window listeners run in the capture phase so the editor, which usually has
 * focus, cannot swallow the keys; losing focus clears the state since the
 * key-up may land in another window. Spread `onPointerMove` on the panel to
 * pick up a modifier pressed while another window had focus.
 */
export function useJumpModifier(): {
    armed: boolean;
    onPointerMove: (e: React.PointerEvent) => void;
} {
    const [armed, setArmed] = useState(false);
    useEffect(() => {
        const onKey = (e: KeyboardEvent) => setArmed(isJumpGesture(e));
        const onBlur = () => setArmed(false);
        window.addEventListener('keydown', onKey, true);
        window.addEventListener('keyup', onKey, true);
        window.addEventListener('blur', onBlur);
        return () => {
            window.removeEventListener('keydown', onKey, true);
            window.removeEventListener('keyup', onKey, true);
            window.removeEventListener('blur', onBlur);
        };
    }, []);
    const onPointerMove = useCallback(
        (e: React.PointerEvent) => setArmed(isJumpGesture(e)),
        [],
    );
    return { armed, onPointerMove };
}

/** Tooltip for a control: why it is out of sync, if it is, plus the jump hint. */
export function controlTooltip(message: string | undefined): string {
    return message ? `${message}\n${JUMP_HINT}` : JUMP_HINT;
}

/** Why a control does not match the running patch, or undefined if it does. */
export function unsyncedMessage(control: ControlBinding): string | undefined {
    if (control.incomplete) {
        return 'Arguments are incomplete — finish editing to use this control';
    }
    if (control.synced) {
        return undefined;
    }
    return control.moduleId !== null
        ? 'Differs from the running patch — evaluate to apply'
        : 'Not in the running patch — evaluate to apply';
}

/** Shown on a control the running patch does not match — one not yet
 *  evaluated, in a buffer that is not running, edited since evaluation, or
 *  with arguments mid-edit. */
export function UnsyncedMarker({ message }: { message: string }) {
    return <span className="control-unsynced" title={message} />;
}
