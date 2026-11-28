import { describe, expect, it } from 'vitest';
import { exec } from './videoExec';

type Result = ReturnType<typeof exec>;

const taps = (patch: Result['patch']) =>
    patch.modules.filter((m) => m.moduleType === '_videoTap');

const sides = (patch: Result['patch']) =>
    patch.modules.filter(
        (m) => m.moduleType === '$left' || m.moduleType === '$right',
    );

/** The module the tap behind history row `row` reads. */
const tapSource = (
    patch: Result['patch'],
    video: NonNullable<Result['video']>,
    row: number,
) => {
    const tap = taps(patch).find(
        (m) => m.params.slot === video.histories[row].tap,
    )!;
    const [cable] = tap.params.input as { module: string }[];
    return patch.modules.find((m) => m.id === cable.module)!;
};

describe('$v.scopeXY', () => {
    it('draws one trace from a stereo signal with the default settings', () => {
        const { patch, video } = exec(`
            $v.scopeXY($stereo($sine('440hz'), $sine('330hz'))).out()
        `);
        expect(video!.scopes).toEqual([
            {
                intensity: { kind: 'const', value: 0.6 },
                lineWidth: { kind: 'const', value: 1 },
                persistence: { kind: 'const', value: 0.6 },
                samples: 2048,
                traces: [{ x: 0, y: 1 }],
                upsample: false,
                xRange: [-5, 5],
                yRange: [-5, 5],
            },
        ]);
        expect(video!.histories).toEqual([
            { samples: 2048, tap: 0, trigger: false },
            { samples: 2048, tap: 1, trigger: false },
        ]);
        expect(taps(patch)).toHaveLength(2);
        expect(video!.wgsl).toContain('var scope_0: texture_2d<f32>');
        expect(video!.wgsl).toContain('video_scope(scope_0, uv)');
    });

    it('draws one trace per lane, each side through its own tap', () => {
        const { patch, video } = exec(`
            $v.scopeXY($stereo($sine(['c3', 'e3', 'g3']), $sine('c2'))).out()
        `);
        const [scope] = video!.scopes;
        expect(scope.traces).toHaveLength(3);
        expect(new Set(scope.traces.map((t) => t.x)).size).toBe(3);
        expect(new Set(scope.traces.map((t) => t.y)).size).toBe(3);
        expect(taps(patch)).toHaveLength(6);
    });

    it('reads x from the left side and y from the right', () => {
        const { patch, video } = exec(`
            $v.scopeXY($stereo($sine('c4'), $saw('c4'))).out()
        `);
        const [trace] = video!.scopes[0].traces;
        expect(tapSource(patch, video!, trace.x).moduleType).toBe('$left');
        expect(tapSource(patch, video!, trace.y).moduleType).toBe('$right');
    });

    it('shares the taps of a signal drawn by several scopes', () => {
        const { patch, video } = exec(`
            const s = $stereo($sine('c4'), $saw('c4'))
            $v.add($v.scopeXY(s), $v.scopeXY(s, { persistence: 0.9 })).out()
        `);
        expect(video!.scopes).toHaveLength(2);
        expect(taps(patch)).toHaveLength(2);
        expect(sides(patch)).toHaveLength(2);
    });

    it('takes an array mixing collections, numbers and notes', () => {
        const { video } = exec(`
            $scopeXY([
                $stereo($sine('c3'), $saw(['c2', 'g2'])),
                2,
                'e3',
                $sine('g3'),
            ])
        `);
        expect(video!.scopes[0].traces).toHaveLength(5);
    });

    it('draws a mono lane on both axes from one tap', () => {
        const { patch, video } = exec(`$v.scopeXY($sine('c4')).out()`);
        const [trace] = video!.scopes[0].traces;
        expect(trace.y).toBe(trace.x);
        expect(tapSource(patch, video!, trace.x).moduleType).toBe('$sine');
        expect(taps(patch)).toHaveLength(1);
        expect(sides(patch)).toHaveLength(0);
    });

    it('points a separate y signal at $stereo', () => {
        expect(() =>
            exec(`$v.scopeXY($sine('c4'), $sine('g4')).out()`),
        ).toThrow(/join two signals with \$stereo\(x, y\)/);
        expect(() => exec(`$scopeXY($sine('c4'), 2.5)`)).toThrow(/\$stereo/);
        expect(() =>
            exec(`$scopeXY($sine('c4'), $sine('g4').items[0])`),
        ).toThrow(/\$stereo/);
        expect(() =>
            exec(`$scopeXY($sine('c4'), $g1([$sine('g4'), $sine('a4')]))`),
        ).toThrow(/\$stereo/);
    });

    it('passes the config through', () => {
        const { video } = exec(`
            $v.scopeXY($stereo($sine('c4'), $sine('g4')), {
                xRange: [-1, 1],
                yRange: [0, 10],
                samples: 512,
                intensity: 0.9,
                persistence: 0.2,
                lineWidth: 2.5,
                upsample: true,
            }).out()
        `);
        expect(video!.scopes[0]).toMatchObject({
            intensity: { kind: 'const', value: 0.9 },
            lineWidth: { kind: 'const', value: 2.5 },
            persistence: { kind: 'const', value: 0.2 },
            samples: 512,
            upsample: true,
            xRange: [-1, 1],
            yRange: [0, 10],
        });
    });

    it('rejects a reversed or non-finite range', () => {
        expect(() =>
            exec(
                `$v.scopeXY($stereo($sine('c4'), $sine('c4')), { xRange: [5, -5] }).out()`,
            ),
        ).toThrow(/xRange/);
        expect(() =>
            exec(
                `$v.scopeXY($stereo($sine('c4'), $sine('c4')), { yRange: [0, Infinity] }).out()`,
            ),
        ).toThrow(/yRange/);
    });

    it('rejects settings out of range', () => {
        expect(() =>
            exec(
                `$v.scopeXY($stereo($sine('c4'), $sine('c4')), { persistence: 2 }).out()`,
            ),
        ).toThrow(/persistence/);
        expect(() =>
            exec(
                `$v.scopeXY($stereo($sine('c4'), $sine('c4')), { samples: 1 }).out()`,
            ),
        ).toThrow(/samples/);
    });

    it('rejects a signal with no lanes', () => {
        expect(() => exec(`$v.scopeXY([]).out()`)).toThrow(
            /\$v\.scopeXY: signal has no lanes/,
        );
    });

    it('drops a scope the picture does not use, with its signals', () => {
        const { patch, video } = exec(`
            $v.scopeXY($stereo($sine('c4'), $sine('g4')))
            $v.osc(4).out()
        `);
        expect(video!.scopes).toEqual([]);
        expect(video!.histories).toEqual([]);
        expect(taps(patch)).toHaveLength(0);
        expect(sides(patch)).toHaveLength(0);
    });

    it('can be warped and composited like any signal', () => {
        const { video } = exec(`
            $v.out($v.mult($v.scopeXY($stereo($sine('c4'), $sine('g4'))).kaleid(4), $v.hsv(0.9, 1, 1)))
        `);
        expect(video!.scopes).toHaveLength(1);
        expect(video!.wgsl).toContain('video_scope(scope_0, uv)');
    });

    it('numbers several scopes and their history rows densely', () => {
        const { video } = exec(`
            const a = $v.scopeXY($stereo($sine('c4'), $sine('g4')))
            const b = $v.scopeXY($stereo($sine('c4'), $sine('e4')), { samples: 1024 })
            $v.add(a, b).out()
        `);
        expect(video!.scopes.map((s) => s.traces)).toEqual([
            [{ x: 0, y: 1 }],
            [{ x: 2, y: 3 }],
        ]);
        expect(video!.wgsl).toContain('var scope_1: texture_2d<f32>');
    });
});

describe('$scopeXY', () => {
    it('shows $v.scopeXY as the picture', () => {
        const { video } = exec(
            `$scopeXY($stereo($sine('440hz'), $sine('330hz')))`,
        );
        expect(video!.hasOutput).toBe(true);
        expect(video!.scopes).toHaveLength(1);
    });

    it('returns the scope so it can be reused', () => {
        const { video } = exec(`
            const s = $scopeXY($stereo($sine('440hz'), $sine('330hz')))
            $v.out($v.mult(s, $v.hsv(0.5, 1, 1)))
        `);
        expect(video!.scopes).toHaveLength(1);
    });
});

describe('$v.scopeXY settings driven by signals', () => {
    it('binds a slider to a uniform slot the scope reads', () => {
        const { video } = exec(`
            $v.scopeXY($stereo($sine('c4'), $sine('g4')), {
                intensity: $slider('Beam', 0.8, 0, 1),
            }).out()
        `);
        const [scope] = video!.scopes;
        expect(scope.intensity).toEqual({ kind: 'uniform', slot: 0 });
        expect(video!.uniforms).toEqual([
            { kind: 'control', moduleId: '__slider_Beam', slot: 0, value: 0.8 },
        ]);
        expect(scope.persistence).toEqual({ kind: 'const', value: 0.6 });
    });

    it('binds an audio signal through a tap', () => {
        const { video } = exec(`
            $v.scopeXY($stereo($sine('c4'), $sine('g4')), {
                lineWidth: $sine('1hz').range(0.5, 3),
            }).out()
        `);
        const [scope] = video!.scopes;
        expect(scope.lineWidth.kind).toBe('uniform');
        const slot = (scope.lineWidth as { slot: number }).slot;
        expect(video!.uniforms[slot].kind).toBe('tap');
    });

    it('keeps the slots of settings when renumbering', () => {
        const { video } = exec(`
            const unused = $v.solid($slider('Unused', 1, 0, 1), 0, 0)
            $v.scopeXY($stereo($sine('c4'), $sine('g4')), {
                intensity: $slider('Beam', 0.8, 0, 1),
            }).out()
        `);
        expect(video!.scopes[0].intensity).toEqual({
            kind: 'uniform',
            slot: 0,
        });
        expect(video!.uniforms).toHaveLength(1);
    });

    it('rejects a video signal', () => {
        expect(() =>
            exec(
                `$v.scopeXY($stereo($sine('c4'), $sine('g4')), { intensity: $v.osc(2) }).out()`,
            ),
        ).toThrow(/intensity takes a number, a slider or an audio signal/);
    });
});
