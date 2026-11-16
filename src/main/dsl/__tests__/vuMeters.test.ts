/**
 * Tests for the VU meter output stage: tracks' mute, solo, gain and pan, the
 * compiled gate chain (__vuTap → __vuSlew → gate
 * $amp driven by a __vuMute $signal), and the `patch.vuMeters`
 * entries the renderer panel consumes.
 */

import { describe, expect, test } from 'vitest';
import type { ModuleSpec, PatchGraph } from '@modular/core';
import schemas from '@modular/core/schemas.json';
import type { VuMeterDef } from '../../../shared/dsl/vuMeterTypes';
import { type DSLExecutionResult, executePatchScript } from '../executor';

const DEFAULT_EXECUTION_OPTIONS = {
    sampleRate: 48_000,
    workspaceRoot: '/workspace',
} as const;

function exec(source: string): DSLExecutionResult {
    return executePatchScript(source, schemas, DEFAULT_EXECUTION_OPTIONS);
}

function execPatch(source: string): PatchGraph {
    return exec(source).patch;
}

function vuMeters(patch: PatchGraph): VuMeterDef[] {
    return patch.vuMeters as unknown as VuMeterDef[];
}

function moduleById(patch: PatchGraph, id: string): ModuleSpec {
    const module = patch.modules.find((m) => m.id === id);
    expect(module, `module ${id} should exist`).toBeDefined();
    return module!;
}

describe('vuMeters emission', () => {
    test('labeled stereo and mono outs emit specs in source order', () => {
        const patch = execPatch(`
            lead$2: $sine('c4')
            bass$0m: $saw('c2')
        `);
        const meters = vuMeters(patch);
        expect(meters).toHaveLength(3);

        expect(meters[0]).toMatchObject({
            baseChannel: 2,
            channels: 2,
            key: 'lead',
            label: 'lead',
            moduleId: '__vuTap_lead',
            mute: false,
            muteModuleId: '__vuMute_lead',
            portName: 'output',
            solo: false,
        });
        expect(meters[1]).toMatchObject({
            baseChannel: 0,
            channels: 1,
            key: 'bass',
            label: 'bass',
            moduleId: '__vuTap_bass',
            muteModuleId: '__vuMute_bass',
        });
        // Source order wins even though compilation is channel-sorted (the
        // bass group compiles first as channel 0 < 2); the end-of-chain
        // master meter always comes last.
        expect(meters.map((m) => m.key)).toEqual(['lead', 'bass', '__main__']);
    });

    test('the end-of-chain master meter taps ROOT_OUTPUT', () => {
        const patch = execPatch(`out: $sine('c4')`);
        const meters = vuMeters(patch);
        const main = meters[meters.length - 1];
        expect(main).toMatchObject({
            channels: 2,
            gain: 2.5,
            gainLocked: false,
            gainModuleId: '__vuGain_main',
            key: '__main__',
            label: 'Main',
            main: true,
            moduleId: 'ROOT_OUTPUT',
            pan: null,
            panModuleId: null,
            portName: 'output',
        });
        expect(main.muteModuleId).toBeUndefined();
        // The lifted master gain seeds the builder's output-gain default.
        expect(moduleById(patch, '__vuGain_main').params.value).toBe(2.5);
    });

    test('$setMasterGain seeds the lifted master gain', () => {
        const patch = execPatch(`
            $setMasterGain(4)
            out: $sine('c4')
        `);
        expect(moduleById(patch, '__vuGain_main').params.value).toBe(4);
        const meters = vuMeters(patch);
        expect(meters[meters.length - 1].gain).toBe(4);
    });

    test('a signal-valued output gain locks the master fader', () => {
        const patch = execPatch(`
            $setMasterGain($sine('0.1hz').range(0, 5))
            out: $sine('c4')
        `);
        const meters = vuMeters(patch);
        const main = meters[meters.length - 1];
        expect(main.gain).toBeNull();
        expect(main.gainModuleId).toBeNull();
        expect(main.gainLocked).toBe(true);
        expect(main.gainSource).toBeDefined();
        expect(patch.modules.some((m) => m.id === '__vuGain_main')).toBe(false);
    });

    test("a track's hidden ids follow its name and stay explicit", () => {
        const patch = execPatch(`
            lead: $sine('c4')
            _bass$0m: $saw('c2')
        `);
        const meters = vuMeters(patch);
        expect(meters.map((m) => m.key)).toEqual(['lead', 'bass', '__main__']);
        expect(meters[1].moduleId).toBe('__vuTap_bass');
        expect(meters[1].muteModuleId).toBe('__vuMute_bass');
        // A track's name is stable identity, so its controls keep their
        // state across a swap.
        expect(moduleById(patch, '__vuMute_bass').idIsExplicit).toBe(true);
        expect(
            moduleById(patch, '__vuGain_lead').skipStateTransfer,
        ).toBeUndefined();
        expect(
            moduleById(patch, '__vuPan_lead').skipStateTransfer,
        ).toBeUndefined();
    });

    test('names are sanitized into module ids', () => {
        const patch = execPatch(`café: $sine('c4')`);
        const meters = vuMeters(patch);
        expect(meters[0].key).toBe('café');
        expect(meters[0].moduleId).toBe('__vuTap_caf_');
    });

    test('no outs produce an empty vuMeters list', () => {
        const patch = execPatch(`$sine('c4')`);
        expect(vuMeters(patch)).toEqual([]);
        const root = moduleById(patch, 'ROOT_OUTPUT');
        expect(root.moduleType).toBe('$signal');
    });

    test("a meter's source location is its track's label", () => {
        const patch = execPatch(`const f = 1\n  lead: $sine('c4')`);
        expect(vuMeters(patch)[0].sourceLocation).toEqual({
            column: 3,
            line: 2,
        });
    });

    test('collection out produces one group', () => {
        const patch = execPatch(`chord: $saw(['c3', 'e3', 'g3'])`);
        const meters = vuMeters(patch).filter((m) => !m.main);
        expect(meters).toHaveLength(1);
        expect(meters[0].channels).toBe(2);
    });
});

describe('pan lifting', () => {
    test('stereo out with no pan lifts a centered pan _smoothControl', () => {
        const patch = execPatch(`lead: $sine('c4')`);
        const pan = moduleById(patch, '__vuPan_lead');
        expect(pan.moduleType).toBe('_smoothControl');
        expect(pan.params.value).toBe(0);
        const meter = vuMeters(patch)[0];
        expect(meter.pan).toBe(0);
        expect(meter.panModuleId).toBe('__vuPan_lead');
        // The out's `$pan` is cabled from the lifted signal.
        const mixers = patch.modules.filter(
            (m) =>
                m.moduleType === '$pan' &&
                JSON.stringify(m.params.pan ?? '').includes('__vuPan_lead'),
        );
        expect(mixers).toHaveLength(1);
    });

    test('a numeric pan option seeds the lifted signal', () => {
        const patch = execPatch(`lead: $sine('c4').track({ pan: -2.5 })`);
        expect(moduleById(patch, '__vuPan_lead').params.value).toBe(-2.5);
        expect(vuMeters(patch)[0].pan).toBe(-2.5);
    });

    test('a signal-valued pan renders locked, with a live tap', () => {
        const patch = execPatch(
            `lead: $sine('c4').track({ pan: $sine('1hz') })`,
        );
        expect(patch.modules.some((m) => m.id === '__vuPan_lead')).toBe(false);
        const meter = vuMeters(patch)[0];
        expect(meter.pan).toBeNull();
        expect(meter.panModuleId).toBeNull();
        expect(meter.panLocked).toBe(true);
        expect(meter.panSource).toBeDefined();
        expect(meter.panSource!.portName).toBe('output');
    });

    test('editable pans carry no lock or tap', () => {
        const patch = execPatch(`lead: $sine('c4')`);
        const meter = vuMeters(patch)[0];
        expect(meter.panLocked).toBe(false);
        expect(meter.panSource).toBeUndefined();
    });

    test('mono outs have no pan control', () => {
        const patch = execPatch(`bass$0m: $saw('c2')`);
        const meter = vuMeters(patch)[0];
        expect(meter.pan).toBeNull();
        expect(meter.panModuleId).toBeNull();
        expect(patch.modules.some((m) => m.id.startsWith('__vuPan_'))).toBe(
            false,
        );
    });
});

describe('gain lifting', () => {
    test('an absent gain lifts a unity gain _smoothControl', () => {
        const patch = execPatch(`lead: $sine('c4')`);
        const gain = moduleById(patch, '__vuGain_lead');
        expect(gain.moduleType).toBe('_smoothControl');
        expect(gain.params.value).toBe(5);
        const meter = vuMeters(patch)[0];
        expect(meter.gain).toBe(5);
        expect(meter.gainModuleId).toBe('__vuGain_lead');
        // The tap is the gain stage itself, fed by the lifted gain.
        const tap = moduleById(patch, '__vuTap_lead');
        expect(tap.moduleType).toBe('$gain');
        expect(JSON.stringify(tap.params.level)).toContain('__vuGain_lead');
    });

    test('a numeric gain option seeds the lifted signal', () => {
        const patch = execPatch(`bass$0m: $saw('c2').track({ gain: 2.5 })`);
        expect(moduleById(patch, '__vuGain_bass').params.value).toBe(2.5);
        expect(vuMeters(patch)[0].gain).toBe(2.5);
    });

    test('a signal-valued gain renders locked, with a live tap', () => {
        const patch = execPatch(
            `lead: $sine('c4').track({ gain: $sine('1hz').range(0, 5) })`,
        );
        expect(patch.modules.some((m) => m.id === '__vuGain_lead')).toBe(false);
        const meter = vuMeters(patch)[0];
        expect(meter.gain).toBeNull();
        expect(meter.gainModuleId).toBeNull();
        expect(meter.gainLocked).toBe(true);
        expect(meter.gainSource).toBeDefined();
        // The gain stage still compiles and carries the tap.
        expect(moduleById(patch, '__vuTap_lead').moduleType).toBe('$gain');
    });
});

describe('track names', () => {
    test('names colliding after sanitization get distinct ids', () => {
        const patch = execPatch(`
            aé: $sine('c4')
            aè: $saw('c2')
        `);
        const meters = vuMeters(patch).filter((m) => !m.main);
        expect(meters.map((m) => m.moduleId)).toEqual([
            '__vuTap_a_',
            '__vuTap_a__2',
        ]);
    });

    test("a 'main' track never collides with the master gain id", () => {
        const patch = execPatch(`main: $sine('c4')`);
        expect(vuMeters(patch)[0].moduleId).toBe('__vuTap_main_2');
        expect(moduleById(patch, '__vuGain_main').moduleType).toBe(
            '_smoothControl',
        );
    });
});

describe('output channel bounds', () => {
    test('a mono out reaches the engine’s last channel', () => {
        const patch = execPatch(`out$63m: $sine('c4')`);
        const source = moduleById(patch, 'ROOT_OUTPUT').params
            .value as unknown[];
        expect(source).toHaveLength(64);
        expect(vuMeters(patch)[0].baseChannel).toBe(63);
    });

    test('a stereo out reaches the last pair', () => {
        const patch = execPatch(`out$62: $sine('c4')`);
        const source = moduleById(patch, 'ROOT_OUTPUT').params
            .value as unknown[];
        expect(source).toHaveLength(64);
    });

    test('channels past the engine width throw', () => {
        expect(() => execPatch(`out$64m: $sine('c4')`)).toThrow(
            'channel must be 0-63',
        );
        // A stereo pair needs its right channel to fit too.
        expect(() => execPatch(`out$63: $sine('c4')`)).toThrow(
            'channel must be 0-62',
        );
    });
});

describe('gate chain compilation', () => {
    test('the tap id always sits on the gain stage', () => {
        const patch = execPatch(`lead: $sine('c4')`);
        expect(moduleById(patch, '__vuTap_lead').moduleType).toBe('$gain');
    });

    test('gate chain: $signal → $slew → $amp fed by the tap', () => {
        const patch = execPatch(`lead: $sine('c4')`);

        const mute = moduleById(patch, '__vuMute_lead');
        expect(mute.moduleType).toBe('$signal');
        expect(mute.params.value).toBe(5);

        const slew = moduleById(patch, '__vuSlew_lead');
        expect(slew.moduleType).toBe('$slew');
        expect(slew.params.rise).toBe(0.002);
        expect(slew.params.fall).toBe(0.002);
        expect(slew.params.input).toMatchObject([
            { module: '__vuMute_lead', type: 'cable' },
        ]);
        // The gate must sit at this patch's mute state from a swap's first
        // sample; carrying the old slew level across would leak audio
        // through a still-closing gate.
        expect(slew.skipStateTransfer).toBe(true);

        // The gate stage is a $amp whose input cables come from the tap and
        // whose factor comes from the slewed gate.
        const gates = patch.modules.filter(
            (m) =>
                m.moduleType === '$amp' &&
                JSON.stringify(m.params.factor ?? '').includes('__vuSlew_lead'),
        );
        expect(gates).toHaveLength(1);
        expect(JSON.stringify(gates[0].params.input)).toContain('__vuTap_lead');
    });

    test('mute compiles the gate to 0', () => {
        const patch = execPatch(`_lead: $sine('c4')`);
        expect(moduleById(patch, '__vuMute_lead').params.value).toBe(0);
        expect(vuMeters(patch)[0].mute).toBe(true);
    });

    test('a solo elsewhere gates non-soloed outputs to 0', () => {
        const patch = execPatch(`
            a: $sine('c4')
            S_b: $saw('c2')
        `);
        expect(moduleById(patch, '__vuMute_a').params.value).toBe(0);
        expect(moduleById(patch, '__vuMute_b').params.value).toBe(5);
    });

    test('mute composes with a user gain (both stages present)', () => {
        const patch = execPatch(`_lead: $sine('c4').track({ gain: 2 })`);
        // Tap = the gain stage; the gate stage reads from it.
        expect(moduleById(patch, '__vuTap_lead').moduleType).toBe('$gain');
        expect(moduleById(patch, '__vuMute_lead').params.value).toBe(0);
        expect(patch.modules.some((m) => m.moduleType === '$gain')).toBe(true);
    });

    test('a fresh execution resets ordinals and labels', () => {
        const first = execPatch(`a: $sine('c4')`);
        expect(vuMeters(first).filter((m) => !m.main)).toHaveLength(1);
        // The same label compiles again — builder state does not leak
        // between executions.
        const second = execPatch(`a: $sine('c4')`);
        expect(vuMeters(second)[0].moduleId).toBe('__vuTap_a');
        const third = execPatch(`b: $sine('c4')`);
        expect(vuMeters(third)[0].moduleId).toBe('__vuTap_b');
    });
});

describe('$trackTap', () => {
    /** The cable(s) feeding the `probe` $signal's value. */
    function probe(
        patch: PatchGraph,
    ): { lane: number; module: string; port: string }[] {
        const source = moduleById(patch, 'probe').params.value;
        return (Array.isArray(source) ? source : [source]) as {
            lane: number;
            module: string;
            port: string;
        }[];
    }

    test('the tap is its post-gate audio, one lane', () => {
        const patch = execPatch(`
            $signal($trackTap('bass'), { id: 'probe' })
            bass: $saw('c2')
        `);
        expect(probe(patch)).toHaveLength(1);
        const [lane] = probe(patch);
        expect(lane).toMatchObject({ lane: 0 });
        const gate = moduleById(patch, lane.module);
        expect(gate.moduleType).toBe('$amp');
        expect(gate.params.factor).toMatchObject([{ module: '__vuSlew_bass' }]);
    });

    test('a mono out taps one lane', () => {
        const patch = execPatch(`
            bass$0m: $saw('c2')
            $signal($trackTap('bass'), { id: 'probe' })
        `);
        expect(probe(patch)).toHaveLength(1);
    });

    test('.active taps the mute/solo gate', () => {
        const patch = execPatch(`
            $signal($trackTap('bass').active, { id: 'probe' })
            bass: $saw('c2')
            S_lead: $sine('c4')
        `);
        expect(probe(patch)[0]).toMatchObject({
            module: '__vuMute_bass',
            port: 'output',
        });
        expect(moduleById(patch, '__vuMute_bass').params.value).toBe(0);
    });

    test('.level and .panning tap the lifted controls', () => {
        const patch = execPatch(`
            const tap = $trackTap('bass')
            bass: $saw('c2').track({ gain: 3, pan: -2 })
            $signal(tap.level, { id: 'probe' })
            $signal(tap.panning, { id: 'panProbe' })
        `);
        expect(probe(patch)[0]).toMatchObject({ module: '__vuGain_bass' });
        expect(moduleById(patch, 'panProbe').params.value).toMatchObject([
            { module: '__vuPan_bass' },
        ]);
    });

    /**
     * The `$signal` a tap control wraps a source in: the source's first
     * lane, and the range the wrapper declares.
     */
    function wrapped(patch: PatchGraph, cable: unknown) {
        const [{ module }] = (Array.isArray(cable) ? cable : [cable]) as {
            module: string;
        }[];
        const wrapper = moduleById(patch, module);
        expect(wrapper.moduleType).toBe('$signal');
        const { max, min, value } = wrapper.params;
        return { max, min, value };
    }

    test('other gain and pan signals are tapped through a ranged $signal', () => {
        const patch = execPatch(`
            const g = $signal(4, { id: 'g' })
            const p = $signal(1, { id: 'p' })
            bass: $saw('c2').track({ gain: g, pan: p })
            $signal($trackTap('bass').level, { id: 'probe' })
            $signal($trackTap('bass').panning, { id: 'panProbe' })
        `);
        expect(wrapped(patch, probe(patch))).toMatchObject({
            max: 10,
            min: 0,
            value: { module: 'g' },
        });
        expect(
            wrapped(patch, moduleById(patch, 'panProbe').params.value),
        ).toMatchObject({ max: 5, min: -5, value: { module: 'p' } });
    });

    test('a mono out pans center', () => {
        const patch = execPatch(`
            bass$0m: $saw('c2')
            $signal($trackTap('bass').panning, { id: 'probe' })
        `);
        expect(wrapped(patch, probe(patch)).value).toBe(0);
    });

    /** The `$remap` params behind a probe on `x.range(...)`. */
    function remapParams(patch: PatchGraph, id: string) {
        const [cable] = moduleById(patch, id).params.value as {
            module: string;
        }[];
        const remap = moduleById(patch, cable.module);
        expect(remap.moduleType).toBe('$remap');
        return remap.params;
    }

    test('properties map from the ranges their controls declare', () => {
        const patch = execPatch(`
            bass: $saw('c2')
            $signal(bass.active.range(-1, 1), { id: 'active' })
            $signal(bass.level.range(-1, 1), { id: 'level' })
            $signal(bass.panning.range(-1, 1), { id: 'pan' })
        `);
        const sources = {
            active: '__vuMute_bass',
            level: '__vuGain_bass',
            pan: '__vuPan_bass',
        };
        for (const [id, module] of Object.entries(sources)) {
            const remap = remapParams(patch, id);
            expect(remap).not.toHaveProperty('inMin');
            expect(remap).not.toHaveProperty('inMax');
            expect(remap.input).toMatchObject([{ module }]);
        }
        expect(moduleById(patch, '__vuMute_bass').params).toMatchObject({
            max: 5,
            min: 0,
        });
        expect(moduleById(patch, '__vuGain_bass').params).toMatchObject({
            max: 10,
            min: 0,
        });
        expect(moduleById(patch, '__vuPan_bass').params).toMatchObject({
            max: 5,
            min: -5,
        });
    });

    test('a ranged gain signal is tapped, lending .level its range', () => {
        const patch = execPatch(`
            bass: $saw('c2').track({ gain: $sine('1hz').range(1, 4) })
            $signal($trackTap('bass').level.range(-1, 1), { id: 'level' })
        `);
        const level = remapParams(patch, 'level');
        expect(level).not.toHaveProperty('inMin');
        const [cable] = level.input as { module: string }[];
        expect(moduleById(patch, cable.module).moduleType).toBe('$remap');
    });

    test('a sequenced gain reads its $cycle', () => {
        const patch = execPatch(`
            bass: $saw('c2').track({ gain: $('1 4') })
            $signal($trackTap('bass').level, { id: 'probe' })
        `);
        const cycles = patch.modules.filter((m) => m.moduleType === '$cycle');
        expect(cycles).toHaveLength(1);
        expect(wrapped(patch, probe(patch)).value).toMatchObject({
            module: cycles[0].id,
        });
    });

    test('several gain lanes read as the first', () => {
        const patch = execPatch(`
            const a = $sine('1hz').range(1, 2)
            const b = $signal(4, { id: 'b' })
            bass: $saw('c2').track({ gain: [a, b] })
            mid: $saw('c3').track({ gain: $g1([b, a]) })
            $signal($trackTap('bass').level, { id: 'bassProbe' })
            $signal($trackTap('mid').level, { id: 'midProbe' })
        `);
        const [first] = moduleById(patch, 'bassProbe').params.value as {
            module: string;
        }[];
        expect(moduleById(patch, first.module).moduleType).toBe('$remap');
        expect(
            wrapped(patch, moduleById(patch, 'midProbe').params.value).value,
        ).toMatchObject({ module: 'b' });
    });

    test('a mono out taps its gain', () => {
        const patch = execPatch(`
            bass$0m: $saw('c2').track({ gain: 2 })
            $signal($trackTap('bass').level, { id: 'probe' })
        `);
        expect(probe(patch)[0]).toMatchObject({ module: '__vuGain_bass' });
        expect(moduleById(patch, '__vuGain_bass').params.value).toBe(2);
    });

    test('a deferred gain is tapped once it is set', () => {
        const patch = execPatch(`
            const gain = $deferred()
            bass: $saw('c2').track({ gain })
            $signal($trackTap('bass').level, { id: 'probe' })
            gain.set($signal(3, { id: 'g' }))
        `);
        expect(wrapped(patch, probe(patch)).value).toMatchObject({
            module: 'g',
        });
    });

    test('a deferred ranged gain lends .level its range', () => {
        const patch = execPatch(`
            const gain = $deferred()
            bass: $saw('c2').track({ gain })
            $signal($trackTap('bass').level.range(-1, 1), { id: 'level' })
            gain.set($sine('1hz').range(1, 4))
        `);
        const [cable] = remapParams(patch, 'level').input as {
            module: string;
        }[];
        expect(moduleById(patch, cable.module).moduleType).toBe('$remap');
    });

    test('a tap control can feed its own out', () => {
        const patch = execPatch(`
            bass: $saw('c2').amp(bass.level).track({ gain: 3 })
        `);
        const amp = patch.modules.find(
            (m) => m.moduleType === '$amp' && !m.id.startsWith('__'),
        )!;
        expect(amp.params.factor).toMatchObject([{ module: '__vuGain_bass' }]);
    });

    test('taps of one out share its sources', () => {
        const patch = execPatch(`
            const g = $signal(4, { id: 'g' })
            bass: $saw('c2').track({ gain: g })
            mid: $saw('c3').track({ gain: g })
            $signal($trackTap('bass').level, { id: 'one' })
            $signal($trackTap('bass').level, { id: 'two' })
            $signal($trackTap('mid').level, { id: 'three' })
        `);
        const [one, two, three] = ['one', 'two', 'three'].map(
            (id) => moduleById(patch, id).params.value,
        );
        expect(two).toEqual(one);
        expect(three).not.toEqual(one);
    });

    test('a pattern gain plays through its $cycle', () => {
        const patch = execPatch(`
            bass: $saw('c2').track({ gain: $('1 4') })
            $signal($trackTap('bass').level, { id: 'probe' })
        `);
        expect(
            patch.modules.filter((m) => m.moduleType === '$cycle').length,
        ).toBe(1);
    });

    test('slices keep the tap properties', () => {
        const patch = execPatch(`
            bass: $saw('c2')
            const [left] = $trackTap('bass').chunk(1)
            $signal(left.active, { id: 'probe' })
            $signal(left, { id: 'audio' })
        `);
        expect(probe(patch)[0]).toMatchObject({ module: '__vuMute_bass' });
        expect(moduleById(patch, 'audio').params.value).toHaveLength(1);
    });

    test('a control driven by a tap meters the resolved source', () => {
        const patch = execPatch(`
            bass: $saw('c2')
            lead: $sine('c5').track({ pan: $trackTap('bass').panning })
        `);
        const lead = vuMeters(patch).find((m) => m.key === 'lead')!;
        expect(lead.panSource).toEqual({
            lane: 0,
            moduleId: '__vuPan_bass',
            portName: 'output',
        });
    });

    test('a name no track has throws', () => {
        expect(() =>
            exec(`
                lead: $sine('c4')
                $signal($trackTap('laed'), { id: 'probe' })
            `),
        ).toThrow(/no track is named "laed"/);
    });

    test('an empty name throws', () => {
        expect(() => exec(`$trackTap('')`)).toThrow(/non-empty string/);
    });
});
