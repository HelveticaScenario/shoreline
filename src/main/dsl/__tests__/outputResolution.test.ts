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

type Cable = { module: string; port: string; lane?: number };

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
        expect(ports(`out: $saw($('c e g'))`, '$saw', 'freq')).toEqual(['cv']);
    });

    test('a trig input takes the cycle trig', () => {
        expect(ports(`out: $perc($('c e g'))`, '$perc', 'trigger')).toEqual([
            'trig',
        ]);
    });

    test('a gate input takes the cycle gate', () => {
        expect(ports(`out: $adsr($('c e g'))`, '$adsr', 'gate')).toEqual([
            'gate',
        ]);
    });

    test('a control input takes the default, not another cv output', () => {
        expect(
            ports(`out: $sine('c').amp($('0 2 5'))`, '$amp', 'factor'),
        ).toEqual(['cv']);
    });

    test('a trig input takes the first trigger of the clock', () => {
        expect(ports(`out: $perc($clock)`, '$perc', 'trigger')).toEqual([
            'barTrigger',
        ]);
    });

    test('a gate input falls back to a trigger', () => {
        expect(ports(`out: $adsr($clock)`, '$adsr', 'gate')).toEqual([
            'barTrigger',
        ]);
    });

    test('a trig input falls back to a gate', () => {
        expect(
            ports(
                `out: $perc($coinFlip($clock._beatTrigger, 2.5))`,
                '$perc',
                'trigger',
            ),
        ).toEqual(['heads']);
    });

    test('a cv input takes the default', () => {
        expect(ports(`out: $saw($clock)`, '$saw', 'freq')).toEqual([
            'playhead',
        ]);
    });

    test('with nothing matching, the default is used', () => {
        expect(
            ports(
                `out: $sine($coinFlip($clock._beatTrigger, 2.5))`,
                '$sine',
                'freq',
            ),
        ).toEqual(['heads']);
    });

    test('a config-object input resolves by its own type', () => {
        expect(
            ports(
                `const seq = $('c e')\nout: $saw(seq, { sync: seq })`,
                '$saw',
                'sync',
            ),
        ).toEqual(['trig']);
    });
});

describe('explicit outputs', () => {
    test('a named output is never re-resolved', () => {
        expect(ports(`out: $perc($('c e g')._cv)`, '$perc', 'trigger')).toEqual(
            ['cv'],
        );
        expect(ports(`out: $saw($('c e g')._trig)`, '$saw', 'freq')).toEqual([
            'trig',
        ]);
    });

    test('the default output has a named property', () => {
        expect(
            ports(`out: $perc($clock._playhead)`, '$perc', 'trigger'),
        ).toEqual(['playhead']);
    });
});

describe('arrays', () => {
    test('roots collected in an array resolve per element', () => {
        const source = `const seqs = []
for (let i = 0; i < 3; i++) {
    seqs.push($('c e g'))
}
out: $sine(seqs).amp($perc(seqs))`;
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
        expect(ports(`out: $('0*8').perc()`, '$perc', 'trigger')).toEqual([
            'trig',
        ]);
        expect(ports(`out: $('0*8').adsr()`, '$adsr', 'gate')).toEqual([
            'gate',
        ]);
        expect(ports(`out: $('c e g').saw()`, '$saw', 'freq')).toEqual(['cv']);
    });

    test('a chain method reads its receiver as cv', () => {
        expect(ports(`out: $('c e g').amp(2)`, '$amp', 'input')).toEqual([
            'cv',
        ]);
    });
});

describe('chain method options', () => {
    test('an option resolves by its own input', () => {
        expect(
            ports(
                `out: $sine('c').erratic(1, { trigger: $('c e') })`,
                '$erratic',
                'trigger',
            ),
        ).toEqual(['trig']);
    });
});

describe('a root read as a signal itself', () => {
    test('is the output a cv input takes', () => {
        expect(ports(`out: $sine($('c e')[0])`, '$sine', 'freq')).toEqual([
            'cv',
        ]);
    });
});

describe('a scoped root', () => {
    function scopedPorts(source: string): string[] {
        return run(source).patch.scopes.flatMap((s) =>
            s.lanes.map((l) => l.portName),
        );
    }

    test('draws the output its input takes', () => {
        expect(scopedPorts('out: $perc($`0`.scope())')).toEqual(['trig']);
    });

    test('draws the output it is first resolved to', () => {
        expect(
            scopedPorts(
                'const seq = $`c e`.scope()\nout: $adsr(seq)\nout2: $saw(seq)',
            ),
        ).toEqual(['gate']);
    });

    test('draws the output it was resolved to before the scope', () => {
        expect(
            scopedPorts('const seq = $`0`\nout: $perc(seq)\nseq.scope()'),
        ).toEqual(['trig']);
    });

    test('is recorded only where it is plugged in', () => {
        expect(
            run('out: $perc($`0`.scope())').outputResolutions.map(
                ({ slot, output }) => `${slot} ${output}`,
            ),
        ).toEqual(['trigger trig']);
    });

    test('never plugged in, draws the output it reads as itself', () => {
        expect(scopedPorts('$`c e`.scope()')).toEqual(['cv']);
    });

    test('read by name, draws that output', () => {
        expect(scopedPorts('out: $perc($`0`.scope()._gate)')).toEqual(['gate']);
    });

    test('a slice of it plugged in takes it too', () => {
        expect(
            scopedPorts('const seq = $`[c,e]`.scope()\nout: $perc(seq[1])'),
        ).toEqual(['trig', 'trig']);
    });

    test('joined with another and plugged in takes both', () => {
        expect(
            scopedPorts(
                'const a = $`c`.scope()\nconst b = $`e`.scope()\nout: $adsr(a.concat(b))',
            ),
        ).toEqual(['gate', 'gate']);
    });

    test('a named output draws that output', () => {
        expect(scopedPorts('out: $perc($`0`._gate.scope())')).toEqual(['gate']);
    });
});

describe('a call that throws', () => {
    test('leaves the next call recording as usual', () => {
        const { outputResolutions } = run(
            `const seq = $('c e')\ntry { $sine('c').amp('nope') } catch {}\nout: $adsr(seq)`,
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
            records(`const seq = $('c e g')\nout: $saw(seq).amp($perc(seq))`),
        ).toEqual(['2 freq cv', '2 trigger trig']);
    });

    test('array elements record their index', () => {
        expect(
            records(`const a = $('c')\nconst b = $('e')\nout: $perc([a, b])`),
        ).toEqual(['3 trigger.0 trig', '3 trigger.1 trig']);
    });

    test('. records the chained module param', () => {
        expect(records(`out: $('0*8').perc()`)).toEqual(['1 trigger trig']);
    });

    test('chain methods record the receiver and argument index', () => {
        expect(
            records(
                `const seq = $('c e g')\nout: seq.range(0, 1, -5, 5)\nout2: $sine('1hz').range(seq, 5)`,
            ),
        ).toEqual(['2 receiver cv', '3 0 cv']);
    });

    test('a receiver that only reads outputs is recorded', () => {
        expect(records(`const seq = $('c e g')\nout: seq`)).toEqual([
            '2 receiver cv',
        ]);
    });

    test('named outputs and single-output modules record nothing', () => {
        expect(
            records(
                `const seq = $('c e g')\nout: $saw(seq._cv).amp($perc(seq._trig))`,
            ),
        ).toEqual([]);
    });

    test('a helper function records at its own call sites', () => {
        expect(
            records(
                `function voice(s) {\n    return $saw(s).amp($perc(s))\n}\nout: voice($('c e g'))`,
            ),
        ).toEqual(['2 freq cv', '2 trigger trig']);
    });
});
