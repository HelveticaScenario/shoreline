import { describe, expect, test } from 'vitest';

import schemas from '@modular/core/schemas.json';
import { buildLibSource } from '../../../main/dsl/typescriptLibGen';
import {
    formatOutputMeta,
    formatPlainMeta,
    formatSignalMeta,
    paramMeta,
    withMeta,
} from '../paramRange';

describe('formatSignalMeta', () => {
    test('control shows range and default', () => {
        expect(
            formatSignalMeta(
                {
                    defaultValue: 2.5,
                    maxValue: 5,
                    minValue: 0,
                    signalType: 'control',
                },
                false,
            ),
        ).toBe('0–5, default 2.5');
    });

    test('open and symmetric ranges', () => {
        const meta = { defaultValue: 0, signalType: 'control' };
        expect(formatSignalMeta({ ...meta, minValue: 0 }, true)).toBe('≥ 0');
        expect(formatSignalMeta({ ...meta, maxValue: 5 }, true)).toBe('≤ 5');
        expect(
            formatSignalMeta({ ...meta, maxValue: 5, minValue: -5 }, true),
        ).toBe('±5');
    });

    test('a required param shows no default', () => {
        expect(
            formatSignalMeta(
                {
                    defaultValue: 0,
                    maxValue: 5,
                    minValue: 0,
                    signalType: 'control',
                },
                true,
            ),
        ).toBe('0–5');
    });

    test('pitch reads in notes, frequency in hz', () => {
        expect(
            formatSignalMeta({ defaultValue: 0, signalType: 'pitch' }, false),
        ).toBe('note, default c4');
        expect(
            formatSignalMeta(
                {
                    defaultValue: 0,
                    maxValue: 1,
                    minValue: -1,
                    signalType: 'frequency',
                },
                false,
            ),
        ).toBe('131hz–523hz, default 262hz');
    });

    test('gate and trig name their convention; audio adds nothing', () => {
        expect(
            formatSignalMeta({ defaultValue: 0, signalType: 'gate' }, false),
        ).toBe('gate');
        expect(
            formatSignalMeta({ defaultValue: 0, signalType: 'trig' }, false),
        ).toBe('trigger');
        expect(
            formatSignalMeta({ defaultValue: 0, signalType: 'audio' }, true),
        ).toBeUndefined();
    });
});

describe('formatPlainMeta', () => {
    test('declared bounds and default', () => {
        expect(formatPlainMeta({ default: 6, maximum: 16, minimum: 1 })).toBe(
            '1–16, default 6',
        );
    });

    test("an unsigned integer's implicit floor is hidden", () => {
        expect(
            formatPlainMeta({ format: 'uint32', minimum: 0, type: 'integer' }),
        ).toBeUndefined();
    });

    test('boolean default', () => {
        expect(formatPlainMeta({ default: false, type: 'boolean' })).toBe(
            'default false',
        );
    });
});

test('formatOutputMeta and withMeta', () => {
    expect(formatOutputMeta({ maxValue: 5, minValue: 0, name: 'o' })).toBe(
        '0–5',
    );
    expect(formatOutputMeta({ name: 'o' })).toBeUndefined();
    expect(withMeta('level', '0–5')).toBe('level (0–5)');
    expect(withMeta('level', undefined)).toBe('level');
});

describe('every declared range reaches the generated docs', () => {
    const lib = buildLibSource(schemas);
    // `_`-prefixed modules are internal and not declared in the DSL lib.
    const publicModules = schemas.filter((m) => !m.name.startsWith('_'));

    test('signal params', () => {
        for (const module of publicModules) {
            for (const p of module.signalParams) {
                const meta = paramMeta(module, p.name);
                if (meta) {
                    expect(lib, `${module.name}.${p.name}`).toContain(
                        `(${meta})`,
                    );
                }
            }
        }
    });

    test('main outputs as @returns', () => {
        for (const module of publicModules) {
            const main =
                module.outputs.find((o) => o.default) ?? module.outputs[0];
            const meta = main && formatOutputMeta(main);
            if (meta) {
                expect(lib, module.name).toContain(
                    `@returns ${main.description} (${meta})`,
                );
            }
        }
    });
});
