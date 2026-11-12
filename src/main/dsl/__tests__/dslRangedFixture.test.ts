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
import {
    $r,
    type Collection,
    CollectionWithRange,
    DeferredModuleOutput,
} from '../GraphBuilder';
import { createOutLane } from '../outLane';

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
        ceil: chain(lfo(), 'ceil'),
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
        erraticBlend: chain(lfo(), 'erratic', 2),
        erraticClamp: chain(flipped(), 'erratic', 2, { mode: 'clamp' }),
        clampOverConstant: factory('$clamp')(3, { max: 10, min: 0 }),
        polyErratic: chain(
            (factory('$sine')(['1hz', '3hz']) as Collection).range(0, 2, -5, 5),
            'erratic',
            2,
        ),
        exp: chain(lfo(), 'exp', 2),
        flipped: flipped(),
        floor: chain(flipped(), 'floor'),
        gain: chain(lfo(), 'gain', 2),
        mul: chain(lfo(), 'mul', -2),
        rangeOfRanged: (chain(lfo(), 'add', 1) as CollectionWithRange).range(
            -1,
            1,
        ),
        ranges: $r(lfo(), flipped()),
        round: chain(lfo(), 'round'),
        signal: factory('$signal')(lfo()),
        slew: chain(lfo(), 'slew', 1),
        smoothControl: factory('_smoothControl')(2, { max: 5, min: -5 }),
        sub: chain(lfo(), 'sub', lfo()),
        sah: chain(lfo(), 'sah', unranged()),
        tah: chain(lfo(), 'tah', unranged()),
        wrap: chain(unranged(), 'wrap', -1, 1),
        wet: (chain(lfo(), 'slew', 1) as Collection).wet(2),
        pipeMix: lfo().pipeMix((x) => chain(x, 'slew', 1) as Collection, 2),
    };

    // Lane controls: lifted, so tapped directly, and wrapped around a gain
    // signal without a range of its own.
    chain(factory('$saw')('c2'), 'out', { label: 'lifted' });
    chain(factory('$saw')('c3'), 'out', {
        gain: unranged(),
        label: 'wrapped',
        pan: lfo(),
    });
    for (const label of ['lifted', 'wrapped']) {
        const lane = createOutLane(builder, label);
        chains[`${label}.active`] = lane.active;
        chains[`${label}.level`] = lane.level;
        chains[`${label}.pan`] = lane.pan;
    }

    // A constant channel beside a ranged one has no range to give.
    expect(chain(factory('$sine')(['1hz', '2hz']), 'slew', 1)).toBeInstanceOf(
        CollectionWithRange,
    );
    expect(factory('$slew')([lfo().items[0], 3])).not.toBeInstanceOf(
        CollectionWithRange,
    );
    // A constant has no range for an unset mode to blend within, and a
    // clamp or blend over one holds it within ±5 without giving it a range.
    for (const options of [{}, { mode: 'clamp' }, { mode: 'blend' }]) {
        expect(
            chain(builder.$c('c3'), 'erratic', 2, options),
        ).not.toBeInstanceOf(CollectionWithRange);
    }

    const { modules } = builder.toPatch();

    const ranged = Object.entries(chains).map(([label, result]) => {
        expect(result, label).toBeInstanceOf(CollectionWithRange);
        const outputs = (result as CollectionWithRange).items.map((item) => {
            const output =
                item instanceof DeferredModuleOutput ? item.resolve()! : item;
            return {
                channel: output.channel,
                module: output.moduleId,
                port: output.portName,
            };
        });
        return { label, outputs };
    });
    return { modules, ranged };
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
