import { describe, expect, test } from 'vitest';

import * as prettier from 'prettier/standalone';
import * as prettierBabel from 'prettier/plugins/babel';
import * as prettierEstree from 'prettier/plugins/estree';

import { docHints } from '../apiTypeReader';
import { syntaxErrors } from '../syntaxCheck';
import { formatControlQuickFix } from '../controlFixFormat';
import { computeControlQuickFixes } from '../controlQuickFix';
import {
    addFix,
    addTitles,
    API,
    apply,
    LAYOUT,
    fixesAt,
    wrapFix,
} from './controlQuickFixHarness';

const PRETTIER = {
    printWidth: 60,
    semi: false,
    singleQuote: true,
    tabWidth: 2,
    trailingComma: 'all' as const,
};

/** Whether Babel, a parser independent of the one the fixes are checked
 *  with, accepts `source`. */
async function parsesAsJs(source: string): Promise<boolean> {
    try {
        await prettier.format(source, {
            parser: 'babel',
            plugins: [prettierBabel, prettierEstree as never],
        });
        return true;
    } catch {
        return false;
    }
}

describe('computeControlQuickFixes — API methods and functions', () => {
    test('an out() option wraps with the output level range', () => {
        const { source, fixes } = fixesAt(`$sine('c4').out({ gain: 2|.5 })`);
        expect(apply(source, wrapFix(fixes))).toBe(
            `$sine('c4').out({ gain: $slider('gain', 2.5, 0, 5) })`,
        );
    });

    test('out() adds its options as a new object argument', () => {
        const { source, fixes } = fixesAt(`$sine('c4').out(|)`);
        expect(addTitles(fixes)).toEqual([
            `Add slider for 'gain'`,
            `Add slider for 'pan'`,
            `Add slider for 'spread'`,
        ]);
        expect(apply(source, addFix(fixes, 'pan'))).toBe(
            `$sine('c4').out({ pan: $slider('pan', 0, -5, 5) })`,
        );
    });

    test('send() fills its gain positionally', () => {
        const { source, fixes } = fixesAt(`$sine('c4').send(bus|)`);
        expect(apply(source, addFix(fixes, 'gain'))).toBe(
            `$sine('c4').send(bus, $slider('gain', 5, 0, 5))`,
        );
    });

    test('outMono() sets its gain through the option object', () => {
        const { source, fixes } = fixesAt(`$sine('c4').outMono(0|)`);
        expect(apply(source, addFix(fixes, 'gain'))).toBe(
            `$sine('c4').outMono(0, { gain: $slider('gain', 5, 0, 5) })`,
        );
        const wrap = fixesAt(`$sine('c4').outMono(0, 2.|5)`);
        expect(apply(wrap.source, wrapFix(wrap.fixes))).toContain(
            `$slider('gain', 2.5, 0, 5)`,
        );
    });

    test('a global function param wraps', () => {
        const { source, fixes } = fixesAt(`$setOutputGain(2.|5)`);
        expect(apply(source, wrapFix(fixes))).toBe(
            `$setOutputGain($slider('gain', 2.5, 0, 5))`,
        );
    });

    test('a doc-stated range sets the slider range', () => {
        const { source, fixes } = fixesAt(`$phase.bend(1.|5)`);
        expect(apply(source, wrapFix(fixes))).toBe(
            `$phase.bend($slider('amount', 1.5, -5, 5))`,
        );
    });

    test('doc-stated defaults seed added option controls', () => {
        const { source, fixes } = fixesAt(`$c('a3').erratic(1|)`);
        expect(apply(source, addFix(fixes, 'speed'))).toBe(
            `$c('a3').erratic(1, { speed: $slider('speed', 2, 0, 5) })`,
        );
        expect(apply(source, addFix(fixes, 'bias'))).toContain(
            `$slider('bias', 0, -5, 5)`,
        );
    });

    test('a summary naming another param documents none of the first', () => {
        const { fixes } = fixesAt(`$sine('c4').delay(|)`);
        expect(addTitles(fixes)).toEqual([]);
    });

    test('a param with no stated default is wrapped but never added', () => {
        const { source, fixes } = fixesAt(`$sine('c4').range(0|, 5)`);
        expect(apply(source, wrapFix(fixes))).toContain(
            `.range($slider('outMin', 0, 0, 5), 5)`,
        );
        expect(addTitles(fixes)).toEqual([]);
    });

    test('rest arguments wrap', () => {
        const { source, fixes } = fixesAt(`$sine($c(1|, 2))`);
        expect(apply(source, wrapFix(fixes))).toContain(
            `$c($slider('args', 1, 0, 5), 2)`,
        );
    });

    test('non-signal params and control calls offer nothing', () => {
        expect(fixesAt(`$sine('c4').setLanes(2|)`).fixes).toEqual([]);
        expect(fixesAt(`$setTempo(120|)`).fixes).toEqual([]);
        expect(
            fixesAt(`$slider('x', 1|, 0, 5)`).fixes.map((f) => f.kind),
        ).toEqual(['unwrap']);
    });

    test('an unknown option object blocks option adds', () => {
        const { fixes } = fixesAt(`$sine('c4').out(opts|)`);
        expect(addTitles(fixes)).toEqual([]);
    });
});

describe('computeControlQuickFixes — video', () => {
    test('video functions and chains resolve', () => {
        const direct = fixesAt(`$v.out($v.osc(4|, $v.time))`);
        expect(apply(direct.source, wrapFix(direct.fixes))).toContain(
            `$v.osc($slider('freq', 4, 0, 5), $v.time)`,
        );
        const chain = fixesAt(`$v.time.osc(0.|1)`);
        expect(apply(chain.source, wrapFix(chain.fixes))).toContain(
            `.osc($slider('freq', 0.1, 0, 5))`,
        );
        const mix = fixesAt(`$v.ramp().osc(4).wet(1|)`);
        expect(apply(mix.source, wrapFix(mix.fixes))).toContain(
            `.osc(4).wet($slider('mix', 1, 0, 1))`,
        );
    });

    test('a video config takes member defaults from `@param config.*`', () => {
        const { source, fixes } = fixesAt(
            `$v.out($v.ramp('angular', { rotate: 0.125, | }))`,
        );
        expect(addTitles(fixes)).toEqual([
            `Add slider for 'zoom'`,
            `Add slider for 'shiftX'`,
            `Add slider for 'shiftY'`,
        ]);
        expect(apply(source, addFix(fixes, 'zoom'))).toContain(
            `{ rotate: 0.125, zoom: $slider('zoom', 1, 0, 5)`,
        );
        const wrap = fixesAt(`$v.out($v.ramp('angular', { rotate: 0.1|25 }))`);
        expect(apply(wrap.source, wrapFix(wrap.fixes))).toContain(
            `{ rotate: $slider('rotate', 0.125, 0, 5) }`,
        );
    });

    test('a video config is created when the call has none', () => {
        const { source, fixes } = fixesAt(`$v.out($v.ramp('radial'|))`);
        expect(apply(source, addFix(fixes, 'zoom'))).toContain(
            `$v.ramp('radial', { zoom: $slider('zoom', 1, 0, 5) })`,
        );
    });

    test('chains and methods take the defaults of the function they forward to', () => {
        const { source, fixes } = fixesAt(`$v.time.warp({ zoom: 2, | })`);
        expect(apply(source, addFix(fixes, 'rotate'))).toContain(
            `rotate: $slider('rotate', 0, 0, 5)`,
        );
        const osc = fixesAt(`$v.time.osc(0.1, 0|)`);
        expect(addTitles(osc.fixes)).toEqual([`Add slider for 'offset'`]);
        const fold = fixesAt(`$v.time.fold(|)`);
        expect(apply(fold.source, addFix(fold.fixes, 'gain'))).toContain(
            `.fold($slider('gain', 1, 0, 5))`,
        );
        const range = fixesAt(`$v.time.range(0|)`);
        expect(apply(range.source, addFix(range.fixes, 'max'))).toContain(
            `.range(0, $slider('max', 1, 0, 5))`,
        );
    });

    test('chain-only functions and buffers document their own defaults', () => {
        const scroll = fixesAt(`$v.time.scroll(0.1|)`);
        expect(apply(scroll.source, addFix(scroll.fixes, 'y'))).toContain(
            `.scroll(0.1, $slider('y', 0, 0, 5))`,
        );
        const tint = fixesAt(`$v.time.tint(0.3|)`);
        expect(apply(tint.source, addFix(tint.fixes, 'saturation'))).toContain(
            `$slider('saturation', 1, 0, 5)`,
        );
        const read = fixesAt(`$v.buffer().read({ | })`);
        expect(apply(read.source, addFix(read.fixes, 'zoom'))).toContain(
            `zoom: $slider('zoom', 1, 0, 5)`,
        );
    });

    test('a video input without a numeric default is wrapped, never added', () => {
        const { fixes } = fixesAt(`$v.out($v.noise(4, 0, { | }))`);
        expect(addTitles(fixes)).toEqual([]);
    });

    test('a video input written as a number wraps', () => {
        const { source, fixes } = fixesAt(
            `$v.out($v.noise(4, 0, { mapX: 2| }))`,
        );
        expect(apply(source, wrapFix(fixes))).toContain(
            `{ mapX: $slider('mapX', 2, 0, 5) }`,
        );
    });

    test('a video receiver takes the video member of a shared name', () => {
        const range = fixesAt(`$v.out($v.osc(4).range(0.|2, 1))`);
        expect(apply(range.source, wrapFix(range.fixes))).toContain(
            `.range($slider('min', 0.2, 0, 5), 1)`,
        );
        const fold = fixesAt(`$v.out($v.osc(4).fold(2|))`);
        expect(wrapFix(fold.fixes).title).toBe(`Wrap 'gain' in $slider`);
        expect(addTitles(fold.fixes)).toEqual([]);
        const mult = fixesAt(`$v.out($v.osc(4).mult(2|))`);
        expect(wrapFix(mult.fixes).title).toBe(`Wrap 'b' in $slider`);
    });

    test('the family follows consts, channels, and video values', () => {
        const bound = fixesAt(`const o = $v.osc(4)\n$v.out(o.fold(2|))`);
        expect(wrapFix(bound.fixes).title).toBe(`Wrap 'gain' in $slider`);
        const channel = fixesAt(`$v.out($v.time.osc(0.1).r.range(0|, 1))`);
        expect(wrapFix(channel.fixes).title).toBe(`Wrap 'min' in $slider`);
        const value = fixesAt(`$v.out($v.time.range(0|, 1))`);
        expect(wrapFix(value.fixes).title).toBe(`Wrap 'min' in $slider`);
    });

    test('a video call returning audio leaves the video family', () => {
        const { fixes } = fixesAt(`$sine($v.osc(4).toCV().range(0|, 5)).out()`);
        expect(wrapFix(fixes).title).toBe(`Wrap 'outMin' in $slider`);
        const fn = fixesAt(`$sine($v.toCV($v.osc(4)).range(0|, 5)).out()`);
        expect(wrapFix(fn.fixes).title).toBe(`Wrap 'outMin' in $slider`);
    });

    test('an audio chain resolves to its module', () => {
        const { fixes } = fixesAt(`$sine('c4').fold(2|).out()`);
        expect(wrapFix(fixes).title).toBe(`Wrap 'amount' in $slider`);
    });
});

describe('docHints', () => {
    test('reads ranges and defaults', () => {
        expect(docHints('wet/dry blend, 0–5 (default 5 = fully wet)')).toEqual({
            defaultValue: 5,
            range: { max: 5, min: 0 },
        });
        expect(docHints('Bend by `amount` (-5..5; 0 is linear)')).toEqual({
            range: { max: 5, min: -5 },
        });
        expect(docHints('±5 makes it one-sided (default 0)')).toEqual({
            defaultValue: 0,
            range: { max: 5, min: -5 },
        });
    });

    test('ignores values with units and hyphenated counts', () => {
        expect(docHints('crossover (V/Oct, default ~120 Hz)')).toEqual({});
        expect(docHints('makeup (default 1V ≈ +4.8dB)')).toEqual({});
        expect(docHints('channel (0-63)')).toEqual({});
    });
});

describe('computeControlQuickFixes — stray and trailing commas', () => {
    test('a positional add after a trailing comma takes it as its separator', () => {
        const { source, fixes } = fixesAt(`$sine(0).lpf('100hz',|).out()`);
        expect(apply(source, addFix(fixes, 'resonance'))).toBe(
            `$sine(0).lpf('100hz', $slider('resonance', 0, 0, 5)).out()`,
        );
    });

    test('stray commas in the edited argument list are dropped', () => {
        const { source, fixes } = fixesAt(`$sine(0).lpf('100hz',,|).out()`);
        expect(apply(source, addFix(fixes, 'resonance'))).toBe(
            `$sine(0).lpf('100hz', $slider('resonance', 0, 0, 5)).out()`,
        );
        const empty = fixesAt(`$saw(,|).out()`);
        expect(apply(empty.source, empty.fixes[0])).toBe(
            `$saw($slider('freq', 'c4', 'c2', 'c6')).out()`,
        );
    });

    test('stray commas in the edited object are dropped', () => {
        const { source, fixes } = fixesAt(
            `$pulse('a3', { width: 1,, |}).out()`,
        );
        expect(apply(source, addFix(fixes, 'pwm'))).toBe(
            `$pulse('a3', { width: 1, pwm: $slider('pwm', 0, -5, 5) }).out()`,
        );
        const empty = fixesAt(`$pulse('a3', {|,}).out()`);
        expect(apply(empty.source, addFix(empty.fixes, 'pwm'))).toBe(
            `$pulse('a3', { pwm: $slider('pwm', 0, -5, 5) }).out()`,
        );
    });

    test('a multi-line argument list gets the new argument on its own line', () => {
        const { source, fixes } = fixesAt(
            `$sine(0).lpf(\n  '100hz',|\n).out()`,
        );
        expect(apply(source, addFix(fixes, 'resonance'))).toBe(
            `$sine(0).lpf(\n  '100hz',\n  $slider('resonance', 0, 0, 5),\n).out()`,
        );
    });

    test('a stray comma elsewhere in the call does not block the fix', () => {
        const { source, fixes } = fixesAt(
            `$pulse('a3',, { width: 2.|5 }).out()`,
        );
        expect(apply(source, wrapFix(fixes))).toContain(
            `{ width: $slider('width', 2.5, 0, 5) }`,
        );
    });

    test('stray commas around a comment are dropped, the comment kept', () => {
        const { source, fixes } = fixesAt(
            `$sine(0).lpf('100hz', /* c */ ,,|).out()`,
        );
        expect(apply(source, addFix(fixes, 'resonance'))).toBe(
            `$sine(0).lpf('100hz', $slider('resonance', 0, 0, 5) /* c */ ).out()`,
        );
    });

    test('a comma inside a comment is never taken for a separator', () => {
        const { source, fixes } = fixesAt(
            `$sine(0).lpf('100hz' /* a, b */,|).out()`,
        );
        expect(apply(source, addFix(fixes, 'resonance'))).toBe(
            `$sine(0).lpf('100hz' /* a, b */, $slider('resonance', 0, 0, 5)).out()`,
        );
    });

    test('formatting repairs the separators Prettier would print', async () => {
        const { source, fixes } = fixesAt(
            `$pulse('a3', { width: 1,, |}).out()`,
        );
        const fix = await formatControlQuickFix(source, addFix(fixes, 'pwm'), {
            ...PRETTIER,
            printWidth: 80,
        });
        expect(apply(source, fix)).toBe(
            `$pulse('a3', { width: 1, pwm: $slider('pwm', 0, -5, 5) }).out()`,
        );
    });
});

describe('control quick-fixes never produce malformed JavaScript', () => {
    const VALID = [
        `const bus = $bus((m) => m.out())\n$pulse('a3', { width: 2.5, pwm: 1 }).lpf('100hz', 2,).out({ gain: 2.5 })\n$sine('c4').send(bus, 1).amp(0.5)`,
        `$sine(0).lpf('100hz', 1.0).wet(2.5).outMono(0, 2.5)\n$setOutputGain(2.5)\n$c('a3').erratic(1, { speed: 1.5, })`,
        `function f() {\n  return $saw(\n    'a3', // pitch\n    { width: 1 /* x */ },\n  ).out()\n}\n$phase.bend(1.5, $phase.fold(1))`,
        `$v.out($v.osc(4, $v.time).osc(0.1).osc(4).wet(1))\n$adsr($pulse('2hz'), { attack: 0.1 }).range(0, 5)`,
    ];
    const BROKEN = [
        `$pulse('a3',, { width: 2.5 },,).out(\n$sine('c4').out({ gain: 2,, })`,
        `$sine(0).lpf('100hz' 2).out({ gain: 1 pan: 0 })\n$saw(,).send(bus,)`,
    ];

    /** Every fix at every cursor position in `source`, raw and formatted. */
    async function everyFix(source: string) {
        const out: { pos: number; title: string; text: string }[] = [];
        for (let pos = 0; pos <= source.length; pos++) {
            for (const fix of computeControlQuickFixes(
                source,
                pos,
                API,
                LAYOUT,
            )) {
                const formatted = await formatControlQuickFix(
                    source,
                    fix,
                    PRETTIER,
                );
                for (const f of [fix, formatted]) {
                    out.push({ pos, text: apply(source, f), title: f.title });
                }
            }
        }
        return out;
    }

    test.each(VALID)('valid source stays valid: %s', async (source) => {
        expect(await parsesAsJs(source)).toBe(true);
        const results = await everyFix(source);
        expect(results.length).toBeGreaterThan(20);
        for (const r of results) {
            if (!(await parsesAsJs(r.text))) {
                expect.fail(
                    `${r.title} at ${r.pos} broke the patch:\n${r.text}`,
                );
            }
        }
    });

    test.each(BROKEN)(
        'broken source gets fixes, none adding an error: %s',
        async (source) => {
            const before = syntaxErrors(source).length;
            const results = await everyFix(source);
            expect(results.length).toBeGreaterThan(10);
            for (const r of results) {
                expect(syntaxErrors(r.text).length).toBeLessThanOrEqual(before);
            }
        },
    );
});
