import React from 'react';
import { toKeyChipGroups } from '../../shared/keybindings/accelerator';
import electronAPI from '../electronAPI';
import './KeyChips.css';

// Glyphs on macOS, text elsewhere.
const PLATFORM = electronAPI.platform === 'darwin' ? 'darwin' : 'other';

/** A binding as key chips, one group per press of a chord. */
export const KeyChips: React.FC<{ binding: string }> = ({ binding }) => (
    <span className="key-chips">
        {toKeyChipGroups(binding, PLATFORM).map((group, gi) => (
            <span key={gi} className="key-chip-group">
                {group.map((key, ki) => (
                    <kbd key={ki} className="key-chip">
                        {key}
                    </kbd>
                ))}
            </span>
        ))}
    </span>
);
