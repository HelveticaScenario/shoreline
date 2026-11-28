import { describe, expect, test } from 'vitest';
import schemas from '@modular/core/schemas.json';
import type { ModuleSchema } from '@modular/core';
import { docsNames, docsTargetForDeclaration } from './definitionDocs';

const names = docsNames(schemas as ModuleSchema[]);
const target = (qualified: string) =>
    docsTargetForDeclaration(qualified, names);

describe('docsTargetForDeclaration', () => {
    test('a module, namespace, global or type opens its own docs', () => {
        expect(target('$sine')).toEqual({
            name: '$sine',
            symbolType: 'module',
        });
        expect(target('$delay')).toEqual({
            name: '$delay',
            symbolType: 'module',
        });
        expect(target('$mi')).toEqual({ name: '$mi', symbolType: 'namespace' });
        expect(target('$setTempo')).toEqual({
            name: '$setTempo',
            symbolType: 'global',
        });
        expect(target('Collection')).toEqual({
            name: 'Collection',
            symbolType: 'type',
        });
    });

    test("a chain method opens its module's docs", () => {
        expect(target('ModuleChain.lpf')).toEqual({
            name: '$lpf',
            symbolType: 'module',
        });
        expect(target('ModuleChainMi.chorus')).toEqual({
            name: '$mi.chorus',
            symbolType: 'module',
        });
        expect(target('RangedCollection.amp')).toEqual({
            name: '$amp',
            symbolType: 'module',
        });
    });

    test("a type's own method opens the type's docs", () => {
        expect(target('Pattern.fast')).toEqual({
            name: 'Pattern',
            symbolType: 'type',
        });
    });

    test('a name no docs cover opens nothing', () => {
        expect(target('__trackBinding')).toBeNull();
        expect(target('ModuleChainNowhere.chorus')).toBeNull();
        expect(target('Internal.thing')).toBeNull();
    });
});
