import { describe, expect, it } from 'vitest';
import {
    VIDEO_CHAIN_EXAMPLES,
    VIDEO_DOCS,
    VIDEO_INTRO,
} from '../../../../shared/dsl/videoDocs';
import type {
    VideoGraph,
    VideoNode,
} from '../../../../shared/video/videoGraph';
import { VIDEO_MODULES } from '../modules';
import { compileVideoGraph } from '../wgslCompiler';
import { shaderErrors } from './gpuValidation';
import { exec } from './videoExec';
import { rampInputs } from './wgslFixtures';

/** Every choice of `params`, one value per param. */
function paramCombinations(
    params: Record<string, { values: readonly string[] }> = {},
): Record<string, string>[] {
    return Object.entries(params).reduce(
        (combos, [name, { values }]) =>
            combos.flatMap((combo) =>
                values.map((value) => ({ ...combo, [name]: value })),
            ),
        [{}],
    );
}

/**
 * A graph of one node of `kind` with `params`, its warped inputs reading a
 * ramp and the rest constant, bound to one of each resource it uses. A
 * feedback read is written back, since every buffer read needs a write.
 */
function moduleGraph(kind: string, params: Record<string, string>): VideoGraph {
    const def = VIDEO_MODULES[kind];
    const node: VideoNode = {
        id: 'm',
        kind,
        params,
        inputs: Object.fromEntries(
            Object.keys(def.inputs).map((name) => [
                name,
                def.warped?.includes(name)
                    ? { kind: 'node', id: 'x' }
                    : { kind: 'const', value: 0.5 },
            ]),
        ),
        ...(def.buffer && { buffer: 0 }),
        ...(def.history && { history: 0 }),
        ...(def.source && { source: 0 }),
        ...(def.scope && { scope: 0 }),
    };
    const write: VideoNode = {
        id: 'w',
        kind: 'feedbackWrite',
        inputs: { input: { kind: 'node', id: 'm' } },
        buffer: 0,
    };
    return {
        hasOutput: true,
        nodes: [
            { id: 'x', kind: 'ramp', inputs: rampInputs },
            node,
            ...(def.buffer === 'read' ? [write] : []),
        ],
        output: def.buffer === 'read' ? 'w' : 'm',
        uniforms: [],
        previews: [],
        histories: [{ samples: 512, tap: 0, trigger: true }],
        sources: [{ kind: 'image', path: 'picture.png' }],
        scopes: [],
    };
}

/** XY scopes need a scope table; the examples cover them. */
const MODULE_CASES = Object.entries(VIDEO_MODULES)
    .filter(([, def]) => !def.scope)
    .flatMap(([kind, def]) =>
        paramCombinations(def.params).map(
            (params) => [kind, JSON.stringify(params), params] as const,
        ),
    );

const EXAMPLES = [
    ...VIDEO_INTRO.examples,
    ...VIDEO_DOCS.flatMap((doc) => doc.examples),
    ...VIDEO_CHAIN_EXAMPLES,
];

describe('WGSL the GPU accepts', () => {
    it.each(MODULE_CASES)('compiles %s with %s', async (kind, _, params) => {
        expect(
            await shaderErrors(compileVideoGraph(moduleGraph(kind, params))),
        ).toEqual([]);
    });

    it.each(EXAMPLES)('compiles the example %s', async (source) => {
        const { video } = exec(source);
        expect(video).not.toBeNull();
        expect(await shaderErrors(video!)).toEqual([]);
    });
});
