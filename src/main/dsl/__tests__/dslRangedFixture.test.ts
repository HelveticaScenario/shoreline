/**
 * Emits the DSL ↔ engine range fixture consumed by the Rust integration test
 * `crates/modular_core/tests/dsl_ranged.rs`: a patch of DSL chains and every
 * output the DSL hands out as ranged. The Rust test builds the patch and
 * checks the engine bounds each of those outputs with a range that swings,
 * the promise `.range(outMin, outMax)` relies on.
 *
 * Run via `yarn gen:dsl-ranged-fixture` to refresh after a change to the
 * chains or the rules.
 */

import { expect, test } from 'vitest';
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import schemas from '@modular/core/schemas.json';
import { DSLContext } from '../factories';
import { $r, type Collection, CollectionWithRange } from '../GraphBuilder';

const FIXTURE_PATH = resolve(
    dirname(fileURLToPath(import.meta.url)),
    '../../../../crates/modular_core/tests/fixtures/dsl_ranged.json',
);

/** Call chain method `method`, which the builder installs at runtime. */
function chain(receiver: unknown, method: string, ...args: unknown[]) {
    return (receiver as Record<string, (...a: unknown[]) => unknown>)[method](
        ...args,
    );
}

function buildFixture() {
    const builder = new DSLContext(schemas).getBuilder();
    const factory = (name: string) => builder.getFactory(name);
    const lfo = () =>
        (factory('$sine')('1hz') as Collection).range(0, 2, -5, 5);
    const flipped = () =>
        (factory('$sine')('3hz') as CollectionWithRange).range(5, 0);
    const unranged = () => chain(builder.$c('c3'), 'lpf', 0);

    const chains: Record<string, unknown> = {
        add: chain(lfo(), 'add', 1),
        addRanged: chain(lfo(), 'add', flipped()),
        amp: chain(lfo(), 'amp', 3),
        ceil: lfo().ceil(),
        channel: lfo()[0],
        clamp: chain(lfo(), 'clamp', { min: 0.5 }),
        clampLimits: factory('$clamp')(unranged(), { max: 1, min: -1 }),
        curve: chain(lfo(), 'curve', 2),
        declared: factory('$signal')(3, { max: 10, min: 0 }),
        declaredOverUnranged: factory('$signal')(unranged(), {
            max: 1,
            min: -1,
        }),
        div: chain(lfo(), 'div', 4),
        erraticBlend: lfo().erratic(2),
        erraticClamp: flipped().erratic(2, { mode: 'clamp' }),
        exp: chain(lfo(), 'exp', 2),
        flipped: flipped(),
        floor: flipped().floor(),
        gain: chain(lfo(), 'gain', 2),
        mul: chain(lfo(), 'mul', -2),
        rangeOfRanged: (chain(lfo(), 'add', 1) as CollectionWithRange).range(
            -1,
            1,
        ),
        ranges: $r(lfo(), flipped()),
        round: lfo().round(),
        signal: factory('$signal')(lfo()),
        slew: chain(lfo(), 'slew', 1),
        smoothControl: factory('_smoothControl')(2, { max: 5, min: -5 }),
        sub: chain(lfo(), 'sub', lfo()),
    };

    const ranged = Object.entries(chains).map(([label, result]) => {
        expect(result, label).toBeInstanceOf(CollectionWithRange);
        const outputs = (result as CollectionWithRange).items.map(
            ({ channel, moduleId, portName }) => ({
                channel,
                module: moduleId,
                port: portName,
            }),
        );
        return { label, outputs };
    });
    return { modules: builder.toPatch().modules, ranged };
}

test('emits the DSL ranged fixture', () => {
    const fixture = buildFixture();
    if (process.env.UPDATE_FIXTURES) {
        writeFileSync(FIXTURE_PATH, `${JSON.stringify(fixture, null, 2)}\n`);
    }
    expect(
        JSON.parse(readFileSync(FIXTURE_PATH, 'utf8')),
        'run `yarn gen:dsl-ranged-fixture`',
    ).toEqual(fixture);
});
