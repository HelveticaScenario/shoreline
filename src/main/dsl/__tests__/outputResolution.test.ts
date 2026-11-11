/**
 * A bare reference to a multi-output module stands for the output that fits
 * the input it is plugged into. A named output is always that output.
 */

import { describe, expect, test } from 'vitest';
import schemas from '@modular/core/schemas.json';
import { executePatchScript } from '../executor';

const EXECUTION_OPTIONS = {
    sampleRate: 48_000,
    workspaceRoot: '/workspace',
} as const;

function run(source: string) {
    return executePatchScript(source, schemas, EXECUTION_OPTIONS);
}

type Cable = { module: string; port: string; channel?: number };

/** The cables feeding `param` of the first module of `moduleType`. */
function cables(source: string, moduleType: string, param: string): Cable[] {
    const module = run(source).patch.modules.find(
        (m) => m.moduleType === moduleType,
    );
    const value = (module!.params as Record<string, unknown>)[param];
    return (Array.isArray(value) ? value : [value]) as Cable[];
}

function ports(source: string, moduleType: string, param: string): string[] {
    return cables(source, moduleType, param).map((c) => c.port);
}

describe('resolution by input type', () => {
    test('a pitch input takes the cycle cv', () => {
        expect(ports(`$saw($p('c e g').c).out()`, '$saw', 'freq')).toEqual([
            'cv',
        ]);
    });

    test('a trig input takes the cycle trig', () => {
        expect(ports(`$perc($p('c e g').c).out()`, '$perc', 'trigger')).toEqual(
            ['trig'],
        );
    });

    test('a gate input takes the cycle gate', () => {
        expect(ports(`$adsr($p('c e g').c).out()`, '$adsr', 'gate')).toEqual([
            'gate',
        ]);
    });

    test('a control input takes the default, not another cv output', () => {
        expect(
            ports(
                `$sine('c').amp($p('0 2 5').c).out()`,
                '$scaleAndShift',
                'scale',
            ),
        ).toEqual(['cv']);
    });

    test('a trig input takes the first trigger of the clock', () => {
        expect(ports(`$perc($clock).out()`, '$perc', 'trigger')).toEqual([
            'barTrigger',
        ]);
    });

    test('a gate input falls back to a trigger', () => {
        expect(ports(`$adsr($clock).out()`, '$adsr', 'gate')).toEqual([
            'barTrigger',
        ]);
    });

    test('a trig input falls back to a gate', () => {
        expect(
            ports(
                `$perc($coinFlip($clock.o.beatTrigger, 2.5)).out()`,
                '$perc',
                'trigger',
            ),
        ).toEqual(['heads']);
    });

    test('a cv input takes the default', () => {
        expect(ports(`$saw($clock).out()`, '$saw', 'freq')).toEqual([
            'playhead',
        ]);
    });

    test('with nothing matching, the default is used', () => {
        expect(
            ports(
                `$sine($coinFlip($clock.o.beatTrigger, 2.5)).out()`,
                '$sine',
                'freq',
            ),
        ).toEqual(['heads']);
    });

    test('a config-object input resolves by its own type', () => {
        expect(
            ports(
                `const seq = $p('c e').c\n$saw(seq, { sync: seq }).out()`,
                '$saw',
                'sync',
            ),
        ).toEqual(['trig']);
    });
});

describe('explicit outputs', () => {
    test('a named output is never re-resolved', () => {
        expect(
            ports(`$perc($p('c e g').c.o.cv).out()`, '$perc', 'trigger'),
        ).toEqual(['cv']);
        expect(
            ports(`$saw($p('c e g').c.o.trig).out()`, '$saw', 'freq'),
        ).toEqual(['trig']);
    });

    test('the default output has a named property', () => {
        expect(
            ports(`$perc($clock.o.playhead).out()`, '$perc', 'trigger'),
        ).toEqual(['playhead']);
    });
});

describe('arrays', () => {
    test('roots collected in an array resolve per element', () => {
        const source = `const seqs = []
for (let i = 0; i < 3; i++) {
    seqs.push($p('c e g').c)
}
$sine(seqs).amp($perc(seqs)).out()`;
        const { patch } = run(source);
        expect(
            patch.modules.filter((m) => m.moduleType === '$cycle'),
        ).toHaveLength(3);
        expect(ports(source, '$sine', 'freq')).toEqual(['cv', 'cv', 'cv']);
        expect(ports(source, '$perc', 'trigger')).toEqual([
            'trig',
            'trig',
            'trig',
        ]);
        expect(cables(source, '$perc', 'trigger').map((c) => c.module)).toEqual(
            cables(source, '$sine', 'freq').map((c) => c.module),
        );
    });
});

describe('chaining', () => {
    test('. resolves the receiver by the chained module', () => {
        expect(ports(`$p('0*8').c.perc().out()`, '$perc', 'trigger')).toEqual([
            'trig',
        ]);
        expect(ports(`$p('0*8').c.adsr().out()`, '$adsr', 'gate')).toEqual([
            'gate',
        ]);
        expect(ports(`$p('c e g').c.saw().out()`, '$saw', 'freq')).toEqual([
            'cv',
        ]);
    });

    test('a chain method reads its receiver as cv', () => {
        expect(
            ports(`$p('c e g').c.amp(2).out()`, '$scaleAndShift', 'input'),
        ).toEqual(['cv']);
    });
});

describe('chain method options', () => {
    test('an option resolves by its own input', () => {
        expect(
            ports(
                `$sine('c').erratic(1, { trigger: $p('c e').c }).out()`,
                '_erratic',
                'trigger',
            ),
        ).toEqual(['trig']);
    });
});

describe('a root read as a signal itself', () => {
    test('is the output a cv input takes', () => {
        expect(ports(`$sine($p('c e').c[0]).out()`, '$sine', 'freq')).toEqual([
            'cv',
        ]);
    });
});

describe('a call that throws', () => {
    test('leaves the next call recording as usual', () => {
        const { outputResolutions } = run(
            `const seq = $p('c e').c\ntry { $sine('c').amp($p('c')) } catch {}\n$adsr(seq).out()`,
        );
        expect(outputResolutions.map((r) => `${r.slot} ${r.output}`)).toEqual([
            'gate gate',
        ]);
    });
});

describe('resolution records', () => {
    function records(source: string) {
        return run(source).outputResolutions.map(
            ({ line, slot, output }) => `${line} ${slot} ${output}`,
        );
    }

    test('module calls record the param each root went into', () => {
        expect(
            records(
                `const seq = $p('c e g').c\n$saw(seq).amp($perc(seq)).out()`,
            ),
        ).toEqual(['2 freq cv', '2 trigger trig']);
    });

    test('array elements record their index', () => {
        expect(
            records(
                `const a = $p('c').c\nconst b = $p('e').c\n$perc([a, b]).out()`,
            ),
        ).toEqual(['3 trigger.0 trig', '3 trigger.1 trig']);
    });

    test('. records the chained module param', () => {
        expect(records(`$p('0*8').c.perc().out()`)).toEqual(['1 trigger trig']);
    });

    test('chain methods record the receiver and argument index', () => {
        expect(
            records(
                `const seq = $p('c e g').c\nseq.amp(2).out()\n$sine('c').amp(seq).out()`,
            ),
        ).toEqual(['2 receiver cv', '3 0 cv']);
    });

    test('a receiver that only reads outputs is recorded', () => {
        expect(records(`const seq = $p('c e g').c\nseq.out()`)).toEqual([
            '2 receiver cv',
        ]);
    });

    test('named outputs and single-output modules record nothing', () => {
        expect(
            records(
                `const seq = $p('c e g').c\n$saw(seq.o.cv).amp($perc(seq.o.trig)).out()`,
            ),
        ).toEqual([]);
    });

    test('a helper function records at its own call sites', () => {
        expect(
            records(
                `function voice(s) {\n    return $saw(s).amp($perc(s))\n}\nvoice($p('c e g').c).out()`,
            ),
        ).toEqual(['2 freq cv', '2 trigger trig']);
    });
});
