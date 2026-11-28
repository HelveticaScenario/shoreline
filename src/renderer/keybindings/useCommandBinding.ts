import { useSyncExternalStore } from 'react';
import { getCommandBinding, subscribeCommandBindings } from './keymap';

/** A command's binding in the installed keymap, kept current as
 *  `keybindings.json` changes. */
export function useCommandBinding(commandId: string): string | undefined {
    return useSyncExternalStore(subscribeCommandBindings, () =>
        getCommandBinding(commandId),
    );
}
