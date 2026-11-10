import { describe, expect, it } from 'vitest';
import { toDocsTarget } from '../docsCommand';

describe('toDocsTarget', () => {
    it('accepts a symbol type and name', () => {
        expect(toDocsTarget({ name: '$sine', symbolType: 'module' })).toEqual({
            name: '$sine',
            symbolType: 'module',
        });
    });

    it('rejects values a keybinding could pass that are not targets', () => {
        expect(toDocsTarget(undefined)).toBeNull();
        expect(toDocsTarget('Signal')).toBeNull();
        expect(toDocsTarget({ name: 'Signal' })).toBeNull();
        expect(
            toDocsTarget({ name: 'Signal', symbolType: 'class' }),
        ).toBeNull();
        expect(toDocsTarget({ name: 3, symbolType: 'type' })).toBeNull();
    });
});
