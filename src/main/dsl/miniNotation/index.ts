/**
 * Public entry point for the TypeScript mini-notation implementation.
 *
 * `$(source)`, `$arrange(...)` and the pattern methods each build a
 * pattern: a wire payload (`ParsedPattern`, `ScalePattern`, …) that `$cycle`'s `pattern` param takes, where the Rust side dispatches
 * on its `__kind` (`SeqPatternSource`) and lowers it to a runtime
 * `Pattern<SeqValue>`.
 *
 * What a patch script holds is a handle on that payload. The handle is the
 * `$cycle` that plays the pattern: every collection member and `$cycle`
 * output reads through one shared `$cycle`, made on first use.
 */

import type {
    AtomValue,
    MiniAST,
    ParsedPattern,
    SourceSpan,
    Located,
} from './ast';
import { collectLeafSpans } from './collectLeafSpans';
import { MiniParseError, parseMini } from './parser';
import { ALIGNMENT_MODES, type AlignmentMode } from '../alignmentModes';
import { captureSourceLocation } from '../captureSourceLocation';
import { lookupArgumentSpan } from '../factories';
import { Collection } from '../GraphBuilder';
import {
    cycleOf,
    isPatternPayload,
    makeCycle,
    type PAYLOAD,
    patternPayload,
    registerPattern,
} from '../patterns';
import {
    GLIDE_CURVES,
    GLIDE_MODES,
    type GlideCurve,
    type GlideMode,
} from '../glideCurves';

export type { MiniAST, ParsedPattern } from './ast';
export { MiniParseError } from './parser';

/**
 * Parse a mini-notation string into a pattern whose payload is a
 * `ParsedPattern`.
 *
 * Entry point for all mini-notation usage in the DSL: every mini-notation
 * literal flows through `$`, called with a string or as a template tag.
 * Examples:
 *
 * ```js
 * $saw($`c4 e4 g4`).out()
 * const bass = $("c2 [c2 g2] c2 e2");
 * $saw(bass).amp($perc(bass)).out()
 * ```
 *
 * A tagged template reads as the untagged template would: its cooked strings
 * with each value interpolated, so `` $`${root} e4` `` and `$(`${root} e4`)`
 * parse the same source.
 *
 * The payload matches the Rust `{ ast, source, all_spans }` shape expected
 * during patch-graph deserialization. It also embeds an `argument_span`
 * captured from the call site so that editor highlighting follows the
 * pattern through `const` indirections.
 *
 * Throws `MiniParseError` if `source` is not a string or fails to parse.
 */
function $Impl(
    source: string | TemplateStringsArray,
    ...values: unknown[]
): Pattern<ParsedPattern> {
    if (isTemplateStrings(source) && source.some((s) => s === undefined)) {
        // A tag receives an invalid escape (`\u` with no code) as undefined.
        throw new MiniParseError('$`...` holds an invalid escape sequence');
    }
    const text = isTemplateStrings(source)
        ? source.reduce((acc, str, i) => acc + String(values[i - 1]) + str)
        : source;
    if (typeof text !== 'string') {
        throw new MiniParseError(
            `$() expects a string argument, got ${typeof text}`,
        );
    }
    const ast: MiniAST = parseMini(text);
    const all_spans = collectLeafSpans(ast);
    const sourceLocation = captureSourceLocation();
    const argument_span = lookupArgumentSpan(sourceLocation, 'source');
    const pattern: ParsedPattern = {
        __kind: 'ParsedPattern',
        ast,
        source: text,
        all_spans,
    };
    if (argument_span) {
        pattern.argument_span = argument_span;
    }
    return patternHandle(pattern);
}

/** The strings array a tag function receives: an array carrying `raw`. */
function isTemplateStrings(value: unknown): value is TemplateStringsArray {
    return (
        Array.isArray(value) && Array.isArray((value as { raw?: unknown }).raw)
    );
}

/** The mini-notation factory: `$(source)` or `` $`source` ``. */
export const $ = $Impl;

// ─── .scale / .trans (scale patterns) ──────────────────────────────────

/** Discriminator strings the Rust `SeqPatternSource` untagged enum matches. */
const SCALE_KIND = 'ScalePattern' as const;
const TRANS_KIND = 'TransPattern' as const;

/**
 * A parsed mini-notation source: a `ParsedPattern` without its kind and
 * call-site span. Carried by pattern-method arguments (`.trans` RHS, `.fast`
 * factor, …).
 */
export interface ParsedPatternPayload {
    ast: MiniAST;
    source: string;
    all_spans: Array<[number, number]>;
}

/** `.trans`: callable, with a method for each alignment mode. */
export type Transpose = ((rhs: string) => ScalePattern<TransPatternPayload>) & {
    [M in AlignmentMode]: (rhs: string) => ScalePattern<TransPatternPayload>;
};

/**
 * Wire shape for `pattern.scale(scale)`: `pattern`'s atoms read as degrees of
 * `scale`, which replaces any scale an inner `.scale(...)` gave a note.
 * `argument_spans` is the wrapped pattern's.
 */
export interface ScalePatternPayload {
    __kind: typeof SCALE_KIND;
    pattern: PatternPayload;
    scale: string;
    argument_spans: Array<SourceSpan>;
}

/**
 * Wire shape for `pattern.trans.<mode>(rhs)` on a scale pattern: `rhs`'s
 * degrees added to `pattern`'s. `argument_spans` is the wrapped pattern's,
 * then the RHS's.
 */
export interface TransPatternPayload {
    __kind: typeof TRANS_KIND;
    pattern: PatternPayload;
    rhs: ParsedPatternPayload;
    mode: AlignmentMode;
    argument_spans: Array<SourceSpan>;
}

/**
 * Methods a scale pattern adds or narrows: `.trans`, and every pattern
 * method returning a scale pattern again.
 */
export interface ScalePatternMethods {
    readonly trans: Transpose;
    fast(factor: number | string): ScalePattern<FastPattern>;
    slow(factor: number | string): ScalePattern<SlowPattern>;
    struct(boolPattern: string): ScalePattern<StructPattern>;
    beat(t: number | string, div: number | string): ScalePattern<BeatPattern>;
    glide(
        g: number | string,
        curve?: GlideCurve,
        mode?: GlideMode,
    ): ScalePattern<GlidePattern>;
}

/**
 * A pattern whose atoms are scale degrees: one made by `.scale(...)`, any
 * pattern method or `.trans` on one, or an arrangement whose every section
 * is one.
 */
export type ScalePattern<T extends PatternPayload = PatternPayload> =
    ScalePatternMethods & Pattern<T>;

/** Whether `payload` is a scale pattern, so it takes `.trans`. */
function isScalePayload(payload: PatternPayload): boolean {
    switch (payload.__kind) {
        case SCALE_KIND:
            return true;
        case ARRANGE_KIND:
            return payload.sections.every((s) => isScalePayload(s.pattern));
        case 'ParsedPattern':
            return false;
        default:
            return isScalePayload(payload.pattern);
    }
}

function parsePayload(source: string): ParsedPatternPayload {
    const ast = parseMini(source);
    const all_spans = collectLeafSpans(ast);
    return { ast, source, all_spans };
}

/** Build a `ScalePatternPayload` around `inner`. */
function makeScale(
    inner: PatternPayload,
    scale: string,
): ScalePattern<ScalePatternPayload> {
    if (typeof scale !== 'string') {
        throw new MiniParseError(
            `scale() expects a scale string, got ${typeof scale}`,
        );
    }
    return patternHandle({
        __kind: SCALE_KIND,
        pattern: inner,
        scale,
        argument_spans: collectPatternSpans(inner, '$(...).scale()'),
    }) as ScalePattern<ScalePatternPayload>;
}

/** `.trans` on the scale pattern `inner`. */
function transposer(inner: PatternPayload): Transpose {
    const apply = (
        mode: AlignmentMode,
        rhs: string,
    ): ScalePattern<TransPatternPayload> => {
        if (typeof rhs !== 'string') {
            throw new MiniParseError(
                `trans.${mode}() expects a string RHS, got ${typeof rhs}`,
            );
        }
        const innerSpans = collectPatternSpans(inner, '$(...).trans()');
        // The argument-span analyzer registers each `.trans` RHS under the
        // chain method's source location, keyed by the param name `rhs`.
        const loc = captureSourceLocation();
        const argSpan = lookupArgumentSpan(loc, 'rhs') ?? { start: 0, end: 0 };
        return patternHandle({
            __kind: TRANS_KIND,
            pattern: inner,
            rhs: parsePayload(rhs),
            mode,
            argument_spans: [...innerSpans, argSpan],
        }) as ScalePattern<TransPatternPayload>;
    };

    // Bare invocation defaults to `.in` (strudel's default alignment).
    const builder = ((rhs: string) => apply('in', rhs)) as Transpose;
    for (const mode of ALIGNMENT_MODES) {
        Object.defineProperty(builder, mode, {
            value: (rhs: string) => apply(mode, rhs),
        });
    }
    return builder;
}

// ─── $arrange ─────────────────────────────────────────────────────────

/** Discriminator string the Rust `SeqPatternSource` untagged enum matches. */
const ARRANGE_KIND = 'ArrangePattern' as const;

/** A pattern usable as an `$arrange` section (and as a `$cycle` argument). */
export type PatternPayload =
    | ParsedPattern
    | ScalePatternPayload
    | TransPatternPayload
    | ArrangePattern
    | FastPattern
    | SlowPattern
    | StructPattern
    | BeatPattern
    | GlidePattern;

/** One `[cycles, pattern]` tuple accepted by `$arrange`. */
export type ArrangeSectionArg = readonly [number, Pattern];

/** One `[cycles, pattern]` tuple of an arrangement of scale patterns. */
export type ScaleArrangeSectionArg = readonly [number, ScalePattern];

/**
 * Wire shape for one arrange section. `cycles` is a positive integer, or the
 * string `'Infinity'` for an infinite tail (JSON can't carry numeric infinity).
 * `pattern` is the embedded section payload — its fields already
 * match the Rust `SeqPatternSource` shape, so the object is embedded verbatim.
 */
export interface ArrangeSectionWire {
    cycles: number | 'Infinity';
    pattern: PatternPayload;
}

/**
 * Wire shape for an arrangement. Recognised by the Rust `SeqPatternSource`
 * untagged enum via `__kind === 'ArrangePattern'`. `argument_spans` is the flat
 * per-source span list (sections in order, sources within each section in
 * order) that `$cycle`'s factory maps to `pattern.0`, `pattern.1`, … for editor
 * highlighting — matching the flat `per_source` order the Rust side builds.
 *
 * An `ArrangePattern` is itself a valid `$arrange` section and `$cycle`
 * argument, so arrangements nest.
 */
export interface ArrangePattern {
    __kind: typeof ARRANGE_KIND;
    sections: Array<ArrangeSectionWire>;
    argument_spans: Array<SourceSpan>;
}

/**
 * Arrange multiple patterns over multiple cycles, mirroring Strudel's
 * `arrange`. Each argument is a `[cycles, pattern]` tuple: `pattern` (any
 * pattern, including a nested `$arrange(...)`) plays for `cycles` cycles,
 * and the sections play back-to-back, looping with period `Σ cycles`.
 *
 * Each scale-pattern section resolves through its own scale, so an
 * arrangement can switch keys between sections; one whose every section is a
 * scale pattern is one too. Cycle counts must be positive integers,
 * except a single trailing section may use `Infinity` to loop forever once
 * reached (later sections would never play and are rejected).
 *
 * The result is a pattern like any other, so arrangements nest as sections
 * of another `$arrange(...)`.
 */
export function $arrange(
    ...sections: ScaleArrangeSectionArg[]
): ScalePattern<ArrangePattern>;
export function $arrange(
    ...sections: ArrangeSectionArg[]
): Pattern<ArrangePattern>;
export function $arrange(
    ...sections: ArrangeSectionArg[]
): Pattern<ArrangePattern> {
    if (sections.length === 0) {
        throw new MiniParseError(
            '$arrange() requires at least one [cycles, pattern] section',
        );
    }

    const wireSections: ArrangeSectionWire[] = [];
    const argument_spans: SourceSpan[] = [];

    sections.forEach((sectionArg, i) => {
        if (!Array.isArray(sectionArg) || sectionArg.length !== 2) {
            throw new MiniParseError(
                `$arrange() section ${i} must be a [cycles, pattern] tuple`,
            );
        }
        const [cyclesRaw, sectionPattern] = sectionArg;
        const pattern = patternPayload(sectionPattern);

        if (typeof cyclesRaw !== 'number' || Number.isNaN(cyclesRaw)) {
            throw new MiniParseError(
                `$arrange() section ${i}: cycles must be a number, got ${typeof cyclesRaw}`,
            );
        }
        let cycles: number | 'Infinity';
        if (cyclesRaw === Infinity) {
            if (i !== sections.length - 1) {
                throw new MiniParseError(
                    '$arrange(): an Infinity section must be the last section ' +
                        '(sections after it can never play)',
                );
            }
            cycles = 'Infinity';
        } else if (!Number.isInteger(cyclesRaw) || cyclesRaw <= 0) {
            throw new MiniParseError(
                `$arrange() section ${i}: cycles must be a positive integer or Infinity, got ${cyclesRaw}`,
            );
        } else {
            cycles = cyclesRaw;
        }

        // Collect each section's flat argument spans in order so the factory can
        // map them to `pattern.0`, `pattern.1`, … (mirroring the Rust per_source).
        argument_spans.push(
            ...collectPatternSpans(pattern, `$arrange() section ${i}`),
        );
        wireSections.push({ cycles, pattern });
    });

    return patternHandle({
        __kind: ARRANGE_KIND,
        sections: wireSections,
        argument_spans,
    });
}

// ─── .fast / .slow / .struct / .beat / .glide (modifiers on every pattern)

/** Discriminator strings the Rust `SeqPatternSource` untagged enum matches. */
const FAST_KIND = 'FastPattern' as const;
const SLOW_KIND = 'SlowPattern' as const;
const STRUCT_KIND = 'StructPattern' as const;
const BEAT_KIND = 'BeatPattern' as const;
const GLIDE_KIND = 'GlidePattern' as const;

/**
 * What a patch script holds for a pattern whose wire payload is `T`: the
 * pattern methods, with the payload itself out of reach.
 */
export type Pattern<T extends PatternPayload = PatternPayload> =
    PatternMethods & { readonly [PAYLOAD]: T };

/**
 * The `.fast(...)` / `.slow(...)` / `.struct(...)` / `.beat(...)` /
 * `.glide(...)` methods of every pattern, mirroring Strudel.
 *
 * `.fast`/`.slow`: the factor is a constant (`2`) or a mini-notation number
 * pattern (`"2 4"` → ×2 then ×4 across the cycle). A non-positive factor
 * (`0` or negative) yields silence, matching the in-string `*` / `/`
 * operators.
 *
 * `.struct`: takes event timing from a boolean mini-notation pattern
 * (`"x ~ x x"`, `"1 0 1"`), sampling values from the source pattern. Falsy
 * (`0`) and rest (`~`) slots are silent.
 *
 * `.beat`: places the source value at beat `t` of a `div`-beat cycle,
 * silence elsewhere. `t` is typically a comma stack (`"0,7,10"` — one onset
 * per index) and wraps modulo `div`; a slot not fully inside the cycle
 * (negative `t`, or fractional `t` within `1` of the cycle end) is silent,
 * matching Strudel. Both arguments accept a constant number or a
 * mini-notation number pattern.
 *
 * `.glide`: gives each note a glide length in cycles. `$cycle` ramps its CV
 * from the previous pitch, starting that many cycles before the note's onset
 * and arriving on it. Sampled at each onset; `0` jumps. Accepts a constant
 * number or a mini-notation number pattern. An optional curve (default
 * `'linear'`) and mode (`'in'`, `'out'` or `'inOut'`, default `'in'`) shape
 * the ramp.
 *
 * `.scale`: reads the pattern's atoms as degrees of a scale, making a
 * `ScalePattern`.
 *
 * `.cycle(...rest)`: shorthand for `$cycle(pattern, ...rest)`, forwarding
 * every argument after the pattern to the `$cycle` factory. The `$cycle` it
 * makes is its own, apart from the one the pattern plays through as a
 * module.
 */
export interface PatternMethods {
    cycle(...rest: unknown[]): unknown;
    scale(scale: string): ScalePattern<ScalePatternPayload>;
    fast(factor: number | string): Pattern<FastPattern>;
    slow(factor: number | string): Pattern<SlowPattern>;
    struct(boolPattern: string): Pattern<StructPattern>;
    beat(t: number | string, div: number | string): Pattern<BeatPattern>;
    glide(
        g: number | string,
        curve?: GlideCurve,
        mode?: GlideMode,
    ): Pattern<GlidePattern>;
}

/**
 * Wire shape for `pattern.fast(factor)`. Recognised by the Rust
 * `SeqPatternSource` untagged enum via `__kind === 'FastPattern'`. `pattern` is
 * the wrapped pattern (any `PatternPayload`, so wrappers chain and nest);
 * `argument_spans` is the wrapped pattern's flat per-source span list, plus the
 * factor's span when the factor is a pattern string. The bare interface is the
 * JSON wire shape; the builders return it behind a `Pattern` handle.
 */
export interface FastPattern {
    __kind: typeof FAST_KIND;
    pattern: PatternPayload;
    /**
     * The speed factor. A bare `number` is a constant (`.fast(2)`) and is *not*
     * highlightable. A `ParsedPatternPayload` (`.fast("2 4")`) is its own
     * highlightable source whose active value lights up as it drives the speed.
     */
    factor: number | ParsedPatternPayload;
    argument_spans: Array<SourceSpan>;
}

/** Wire shape for `pattern.slow(factor)` — the time-inverse of `FastPattern`. */
export interface SlowPattern {
    __kind: typeof SLOW_KIND;
    pattern: PatternPayload;
    /** See {@link FastPattern.factor}. */
    factor: number | ParsedPatternPayload;
    argument_spans: Array<SourceSpan>;
}

/**
 * Wire shape for `pattern.struct(boolPattern)`. Recognised by the Rust
 * `SeqPatternSource` untagged enum via `__kind === 'StructPattern'`.
 * `bool_pattern` is always a parsed mini-notation string and always its own
 * highlightable source (appended after the wrapped pattern's spans).
 */
export interface StructPattern {
    __kind: typeof STRUCT_KIND;
    pattern: PatternPayload;
    bool_pattern: ParsedPatternPayload;
    argument_spans: Array<SourceSpan>;
}

/**
 * Wire shape for `pattern.beat(t, div)`. Recognised by the Rust
 * `SeqPatternSource` untagged enum via `__kind === 'BeatPattern'`. `t` and
 * `div` follow {@link FastPattern.factor}'s const-vs-pattern duality; string
 * arguments append their spans after the wrapped pattern's (t first, then
 * div — matching the Rust `per_source` order).
 */
export interface BeatPattern {
    __kind: typeof BEAT_KIND;
    pattern: PatternPayload;
    t: number | ParsedPatternPayload;
    div: number | ParsedPatternPayload;
    argument_spans: Array<SourceSpan>;
}

/**
 * Wire shape for `pattern.glide(g, curve, mode)`. Recognised by the Rust `SeqPatternSource`
 * untagged enum via `__kind === 'GlidePattern'`. `length` follows
 * {@link FastPattern.factor}'s const-vs-pattern duality and, as a string,
 * appends its span after the wrapped pattern's.
 */
export interface GlidePattern {
    __kind: typeof GLIDE_KIND;
    pattern: PatternPayload;
    length: number | ParsedPatternPayload;
    curve: GlideCurve;
    mode: GlideMode;
    argument_spans: Array<SourceSpan>;
}

/**
 * Collect a pattern's flat per-source argument spans in the same order the Rust
 * side flattens `per_source`: of each `$arrange` section, and of the pattern
 * a pattern method wraps. Throws on a non-pattern value.
 */
function collectPatternSpans(
    pattern: PatternPayload,
    context: string,
): SourceSpan[] {
    if (!isPatternPayload(pattern)) {
        throw new MiniParseError(
            `${context}: pattern must be a $(...) or $arrange(...) value, ` +
                `or a pattern method's result`,
        );
    }
    return 'argument_spans' in pattern
        ? [...pattern.argument_spans]
        : [pattern.argument_span ?? { start: 0, end: 0 }];
}

/**
 * Build the wire payload for a numeric pattern-method argument (`.fast`/
 * `.slow` factor, `.beat` t/div, `.glide` length). A number is carried as a raw constant (not
 * a highlightable source); a string is parsed as full mini-notation (`"2 4"`,
 * `"<1 2>"`, …) into a `ParsedPatternPayload` that the Rust side lowers to a
 * `Pattern<Fraction>` and highlights.
 */
function factorPayload(
    arg: number | string,
    methodName: string,
): number | ParsedPatternPayload {
    if (typeof arg === 'number') {
        if (!Number.isFinite(arg)) {
            throw new MiniParseError(
                `${methodName}() expects a finite number, got ${arg}`,
            );
        }
        return arg;
    }
    return parsePayload(arg);
}

/** Build a `FastPattern` wrapper around `inner`. */
function makeFast(
    inner: PatternPayload,
    factor: number | string,
): Pattern<FastPattern> {
    return patternHandle({
        __kind: FAST_KIND,
        pattern: inner,
        factor: factorPayload(factor, 'fast'),
        argument_spans: factorArgumentSpans(inner, factor, 'fast'),
    });
}

/** Build a `SlowPattern` wrapper around `inner`. */
function makeSlow(
    inner: PatternPayload,
    factor: number | string,
): Pattern<SlowPattern> {
    return patternHandle({
        __kind: SLOW_KIND,
        pattern: inner,
        factor: factorPayload(factor, 'slow'),
        argument_spans: factorArgumentSpans(inner, factor, 'slow'),
    });
}

/** Throw unless `value` is one of `allowed`. */
function expectOneOf<T extends string>(
    value: unknown,
    allowed: ReadonlyArray<T>,
    what: string,
): T {
    if (!allowed.includes(value as T)) {
        throw new MiniParseError(
            `glide() ${what} must be one of ${allowed.map((a) => `'${a}'`).join(', ')}, got ${JSON.stringify(value)}`,
        );
    }
    return value as T;
}

/** Build a `GlidePattern` wrapper around `inner`. */
function makeGlide(
    inner: PatternPayload,
    g: number | string,
    curve: GlideCurve = 'linear',
    mode: GlideMode = 'in',
): Pattern<GlidePattern> {
    return patternHandle({
        __kind: GLIDE_KIND,
        pattern: inner,
        length: factorPayload(g, 'glide'),
        curve: expectOneOf(curve, GLIDE_CURVES, 'curve'),
        mode: expectOneOf(mode, GLIDE_MODES, 'mode'),
        argument_spans: factorArgumentSpans(inner, g, 'glide'),
    });
}

/** Build a `StructPattern` wrapper around `inner`. */
function makeStruct(
    inner: PatternPayload,
    boolPattern: string,
): Pattern<StructPattern> {
    if (typeof boolPattern !== 'string') {
        throw new MiniParseError(
            `struct() expects a mini-notation string, got ${typeof boolPattern}`,
        );
    }
    const innerSpans = collectPatternSpans(inner, '$(...).struct()');
    // The bool pattern is always a string source, so unlike a numeric factor
    // it always appends its own span.
    const loc = captureSourceLocation();
    const boolSpan = lookupArgumentSpan(loc, 'boolPattern') ?? {
        start: 0,
        end: 0,
    };
    return patternHandle({
        __kind: STRUCT_KIND,
        pattern: inner,
        bool_pattern: parsePayload(boolPattern),
        argument_spans: [...innerSpans, boolSpan],
    });
}

/** Build a `BeatPattern` wrapper around `inner`. */
function makeBeat(
    inner: PatternPayload,
    t: number | string,
    div: number | string,
): Pattern<BeatPattern> {
    const innerSpans = collectPatternSpans(inner, '$(...).beat()');
    // Span order must match the Rust `per_source` push order: inner sources,
    // then t (string only), then div (string only).
    const loc = captureSourceLocation();
    const argument_spans = [...innerSpans];
    if (typeof t === 'string') {
        argument_spans.push(
            lookupArgumentSpan(loc, 't') ?? { start: 0, end: 0 },
        );
    }
    if (typeof div === 'string') {
        argument_spans.push(
            lookupArgumentSpan(loc, 'div') ?? { start: 0, end: 0 },
        );
    }
    return patternHandle({
        __kind: BEAT_KIND,
        pattern: inner,
        t: factorPayload(t, 'beat'),
        div: factorPayload(div, 'beat'),
        argument_spans,
    });
}

/**
 * Build a wrapper's flat argument spans: the wrapped pattern's spans, then — for
 * a string (pattern) factor only — the factor's own document span (matching the
 * Rust `per_source` order). A number factor is a constant: no span, no source.
 */
function factorArgumentSpans(
    inner: PatternPayload,
    factor: number | string,
    methodName: 'fast' | 'slow' | 'glide',
): SourceSpan[] {
    const innerSpans = collectPatternSpans(inner, `$(...).${methodName}()`);
    if (typeof factor !== 'string') {
        return innerSpans;
    }
    const loc = captureSourceLocation();
    const argName = methodName === 'glide' ? 'g' : 'factor';
    const factorSpan = lookupArgumentSpan(loc, argName) ?? {
        start: 0,
        end: 0,
    };
    return [...innerSpans, factorSpan];
}

/**
 * The pattern methods. Each name is reserved, so no collection member or
 * module takes it.
 */
export const PATTERN_METHODS = [
    'cycle',
    'fast',
    'slow',
    'struct',
    'beat',
    'glide',
    'scale',
    'trans',
] as const;

/** Whether a `$cycle` has `name`, own or inherited, as a collection member. */
function isCollectionMember(name: string): boolean {
    if (name === 'constructor') {
        return false;
    }
    for (
        let proto: object | null = Collection.prototype;
        proto !== null && proto !== Object.prototype;
        proto = Object.getPrototypeOf(proto)
    ) {
        if (Object.hasOwn(proto, name)) {
            return true;
        }
    }
    return false;
}

/**
 * Whether `prop` reads through a pattern's `$cycle`: a collection member, its
 * lanes (`items`), a lane index, a named output (`_gate`) or iteration.
 */
function isCycleMember(prop: string | symbol): boolean {
    if (typeof prop === 'symbol') {
        return prop === Symbol.iterator;
    }
    return (
        prop === 'items' ||
        /^(0|[1-9]\d*)$/.test(prop) ||
        (prop.startsWith('_') && !prop.startsWith('__')) ||
        isCollectionMember(prop)
    );
}

/**
 * The value a patch script holds for the pattern `payload`: a proxy with the
 * pattern methods (and `.trans` on a scale pattern) that otherwise is the pattern's
 * `$cycle`. Reading a collection member, or using the pattern where a signal
 * belongs, makes that `$cycle`. The payload is what crosses to the engine:
 * `patternPayload` gives it to host code, and the script cannot reach it.
 */
function patternHandle<T extends PatternPayload>(pattern: T): Pattern<T> {
    const shared = {
        beat: (t: number | string, div: number | string) =>
            makeBeat(pattern, t, div),
        cycle: (...rest: unknown[]) => makeCycle(pattern, rest),
        fast: (factor: number | string) => makeFast(pattern, factor),
        glide: (g: number | string, curve?: GlideCurve, mode?: GlideMode) =>
            makeGlide(pattern, g, curve, mode),
        scale: (scale: string) => makeScale(pattern, scale),
        slow: (factor: number | string) => makeSlow(pattern, factor),
        struct: (boolPattern: string) => makeStruct(pattern, boolPattern),
    } satisfies Record<
        Exclude<(typeof PATTERN_METHODS)[number], 'trans'>,
        unknown
    >;
    const methods: object = isScalePayload(pattern)
        ? Object.defineProperty(shared, 'trans', {
              enumerable: true,
              get: () => transposer(pattern),
          })
        : shared;
    const handle = new Proxy(
        {},
        {
            get: (target, prop) => {
                if (Object.hasOwn(methods, prop)) {
                    return Reflect.get(methods, prop);
                }
                if (prop === 'trans') {
                    throw new MiniParseError(
                        '.trans transposes scale degrees, so it needs a scale ' +
                            'pattern: add .scale(...) before it',
                    );
                }
                if (isCycleMember(prop)) {
                    const cycle = cycleOf(pattern) as object;
                    const value: unknown = Reflect.get(cycle, prop, cycle);
                    return typeof value === 'function'
                        ? value.bind(cycle)
                        : value;
                }
                return Reflect.get(target, prop);
            },
            // Host code tells signals apart with `instanceof`, so a pattern
            // passes wherever a collection does.
            getPrototypeOf: () => Collection.prototype,
            // A pattern takes no properties of the script's.
            set: () => false,
            has: (target, prop) =>
                Object.hasOwn(methods, prop) ||
                isCycleMember(prop) ||
                Reflect.has(target, prop),
        },
    ) as Pattern<T>;
    registerPattern(handle, pattern);
    return handle;
}

// `Located` is re-exported for tests that hand-construct ASTs.
export type { AtomValue, Located, SourceSpan };
