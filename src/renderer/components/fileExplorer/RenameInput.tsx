import type { KeyboardEvent, MouseEventHandler } from 'react';

/** Focuses the input with the name selected up to its extension. */
function focusStem(input: HTMLInputElement | null) {
    if (!input) return;
    input.focus();
    const lastDotIndex = input.value.lastIndexOf('.');
    if (lastDotIndex !== -1) {
        input.setSelectionRange(0, lastDotIndex);
    } else {
        input.select();
    }
}

interface RenameInputProps {
    name: string;
    onCommit: (newName: string) => void;
    onCancel: () => void;
    onClick?: MouseEventHandler<HTMLInputElement>;
}

/**
 * The inline name field of a row being renamed. Mount it only while the
 * rename runs: Enter commits, Escape or losing focus cancels.
 */
export function RenameInput({
    name,
    onCommit,
    onCancel,
    onClick,
}: RenameInputProps) {
    const handleKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
        if (e.key === 'Enter') {
            e.stopPropagation();
            onCommit(e.currentTarget.value);
        } else if (e.key === 'Escape') {
            e.stopPropagation();
            onCancel();
        }
    };

    return (
        <input
            ref={focusStem}
            type="text"
            className="rename-input"
            defaultValue={name}
            onKeyDown={handleKeyDown}
            onBlur={onCancel}
            onClick={onClick}
        />
    );
}
