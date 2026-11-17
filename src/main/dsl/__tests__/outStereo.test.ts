/**
 * The out stage keeps each device channel one mono lane, whatever stereo
 * reaches it.
 */

import { describe, expect, test } from 'vitest';
import schemas from '@modular/core/schemas.json';
import { executePatchScript } from '../executor';

function modules(source: string) {
    return executePatchScript(source, schemas, {
        sampleRate: 48_000,
        workspaceRoot: '/workspace',
    }).patch.modules;
}

/** The module `id`'s `field` cable source, as `moduleType`. */
function sourceType(
    patch: ReturnType<typeof modules>,
    id: string,
    field: string,
): string | undefined {
    const params = patch.find((m) => m.id === id)?.params as Record<
        string,
        unknown
    >;
    const value = params?.[field];
    const cable = (Array.isArray(value) ? value[0] : value) as
        | { module?: string }
        | undefined;
    return patch.find((m) => m.id === cable?.module)?.moduleType;
}

describe('the out stage', () => {
    test('the root output folds what reaches it', () => {
        const patch = modules(`out: $saw('c3')`);
        expect(sourceType(patch, 'ROOT_OUTPUT', 'value')).toBe('$mono');
    });

    test('a stereo track plays its panned mix at unity', () => {
        const patch = modules(`lead: $saw('c3')`);
        const tap = patch.find((m) => m.id.startsWith('__vuTap_'))!;
        const mixId = (
            (tap.params as Record<string, unknown>).input as {
                module: string;
            }[]
        )[0].module;
        const mix = patch.find((m) => m.id === mixId)!;
        expect(mix.moduleType).toBe('$setLanes');
        // One summed lane with no other params: nothing scales the mix.
        expect(Object.keys(mix.params).sort()).toEqual(['input', 'lanes']);
        expect((mix.params as Record<string, unknown>).lanes).toBe(1);
        expect(sourceType(patch, mixId, 'input')).toBe('$pan');
    });

    test('a signal gain is one lane, folded for a mono out', () => {
        const patch = modules(
            `out$2m: $saw('c3').track({ gain: $stereo($sine('1hz'), 5) })
             out2: $saw('e3').track({ gain: [$sine('1hz'), $sine('2hz')] })`,
        );
        const taps = patch.filter((m) => m.id.startsWith('__vuTap_'));
        const gainSources = taps.map((tap) =>
            sourceType(patch, tap.id, 'level'),
        );
        expect(gainSources).toEqual(['$setLanes', '$setLanes']);
        const lanes = patch.filter((m) => m.moduleType === '$setLanes');
        expect(
            lanes.some((m) =>
                patch.some(
                    (s) =>
                        s.moduleType === '$mono' &&
                        JSON.stringify(m.params).includes(`"${s.id}"`),
                ),
            ),
        ).toBe(true);
    });
});
