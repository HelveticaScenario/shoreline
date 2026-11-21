/**
 * Tests for formatPath, which turns a buffer identifier into the file:// model
 * URI Monaco uses. Untitled buffers and .sl patches become .mjs so they are
 * treated as ES modules; .js, .mjs and .json are preserved.
 */
import { describe, expect, test } from 'vitest';
import { formatPath } from './monacoHelpers';

describe('formatPath', () => {
    test('preserves .json so JSON buffers keep a JSON model URI', () => {
        expect(formatPath('keybindings.json')).toBe('file:///keybindings.json');
        expect(formatPath('/Users/x/keybindings.json')).toBe(
            'file:///Users/x/keybindings.json',
        );
        expect(formatPath('config.json')).toBe('file:///config.json');
    });

    test('appends .mjs to extensionless / untitled buffers', () => {
        expect(formatPath('untitled-1')).toBe('file:///untitled-1.mjs');
    });

    test('appends .mjs to .sl patches', () => {
        expect(formatPath('/abs/patch.sl')).toBe('file:///abs/patch.sl.mjs');
    });

    test('preserves .js and .mjs extensions', () => {
        expect(formatPath('/abs/patch.js')).toBe('file:///abs/patch.js');
        expect(formatPath('/abs/patch.mjs')).toBe('file:///abs/patch.mjs');
    });
});
