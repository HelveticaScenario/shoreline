import type { ModuleSchema } from '@modular/core';
import type {
    JSONSchema,
    Schemas,
    Schema,
} from '../../shared/dsl/schemaTypeResolver';
import {
    schemaToTypeExpr,
    getEnumVariants,
} from '../../shared/dsl/schemaTypeResolver';
import {
    chainMethodName,
    processModuleSchema,
    qualifiesForChain,
} from './paramsSchema';
import { enoscScalePresets } from '../../shared/dsl/enoscScales';
import { dslModules } from '../../shared/dsl/dslModuleDocs';
import {
    generateVideoChainMembers,
    generateVideoGlobals,
    generateVideoNamespace,
} from './videoLibGen';
import { jsDocComment } from './jsDoc';
import {
    BUS_TRACK_OPTIONS,
    TRACK_OPTIONS,
    renderTrackOptionsJsDocList,
    renderTrackOptionsMembers,
} from '../../shared/dsl/trackOptions';
import { EXPERIMENTAL_NOTICE } from '../../shared/dsl/experimental';
import { ALIGNMENT_MODES } from './alignmentModes';
import { GLIDE_CURVES, GLIDE_MODES, literalUnion } from './glideCurves';
import {
    formatOutputMeta,
    paramMeta,
    withMeta,
} from '../../shared/dsl/paramRange';
import { chooseOutput } from '../../shared/dsl/outputResolutionRule';
import { outputName } from './outputNames';
import {
    readDocText,
    renderDocContentJsDoc,
} from '../../shared/dsl/referenceDocs';
import type { FunctionDoc } from '../../shared/dsl/referenceDocs';
import { GLOBAL_ENTRIES, GLOBALS } from '../../shared/dsl/globalDocs';
import {
    chainDeclaration,
    parseDeclaration,
} from '../../shared/dsl/tsDeclaration';

const BASE_LIB_SOURCE = `
/** The **\`console\`** object provides access to the debugging console (e.g., the Web console in Firefox). */
/**
 * The **\`console\`** object provides access to the debugging console (e.g., the Web console in Firefox).
 *
 * [MDN Reference](https://developer.mozilla.org/docs/Web/API/console)
 */
interface Console {
    /**
     * The **\`console.assert()\`** static method writes an error message to the console if the assertion is false.
     *
     * [MDN Reference](https://developer.mozilla.org/docs/Web/API/console/assert_static)
     */
    assert(condition?: boolean, ...data: any[]): void;
    /**
     * The **\`console.clear()\`** static method clears the console if possible.
     *
     * [MDN Reference](https://developer.mozilla.org/docs/Web/API/console/clear_static)
     */
    clear(): void;
    /**
     * The **\`console.count()\`** static method logs the number of times that this particular call to \`count()\` has been called.
     *
     * [MDN Reference](https://developer.mozilla.org/docs/Web/API/console/count_static)
     */
    count(label?: string): void;
    /**
     * The **\`console.countReset()\`** static method resets counter used with console/count_static.
     *
     * [MDN Reference](https://developer.mozilla.org/docs/Web/API/console/countReset_static)
     */
    countReset(label?: string): void;
    /**
     * The **\`console.debug()\`** static method outputs a message to the console at the 'debug' log level.
     *
     * [MDN Reference](https://developer.mozilla.org/docs/Web/API/console/debug_static)
     */
    debug(...data: any[]): void;
    /**
     * The **\`console.dir()\`** static method displays a list of the properties of the specified JavaScript object.
     *
     * [MDN Reference](https://developer.mozilla.org/docs/Web/API/console/dir_static)
     */
    dir(item?: any, options?: any): void;
    /**
     * The **\`console.dirxml()\`** static method displays an interactive tree of the descendant elements of the specified XML/HTML element.
     *
     * [MDN Reference](https://developer.mozilla.org/docs/Web/API/console/dirxml_static)
     */
    dirxml(...data: any[]): void;
    /**
     * The **\`console.error()\`** static method outputs a message to the console at the 'error' log level.
     *
     * [MDN Reference](https://developer.mozilla.org/docs/Web/API/console/error_static)
     */
    error(...data: any[]): void;
    /**
     * The **\`console.group()\`** static method creates a new inline group in the Web console log, causing any subsequent console messages to be indented by an additional level, until console/groupEnd_static is called.
     *
     * [MDN Reference](https://developer.mozilla.org/docs/Web/API/console/group_static)
     */
    group(...data: any[]): void;
    /**
     * The **\`console.groupCollapsed()\`** static method creates a new inline group in the console.
     *
     * [MDN Reference](https://developer.mozilla.org/docs/Web/API/console/groupCollapsed_static)
     */
    groupCollapsed(...data: any[]): void;
    /**
     * The **\`console.groupEnd()\`** static method exits the current inline group in the console.
     *
     * [MDN Reference](https://developer.mozilla.org/docs/Web/API/console/groupEnd_static)
     */
    groupEnd(): void;
    /**
     * The **\`console.info()\`** static method outputs a message to the console at the 'info' log level.
     *
     * [MDN Reference](https://developer.mozilla.org/docs/Web/API/console/info_static)
     */
    info(...data: any[]): void;
    /**
     * The **\`console.log()\`** static method outputs a message to the console.
     *
     * [MDN Reference](https://developer.mozilla.org/docs/Web/API/console/log_static)
     */
    log(...data: any[]): void;
    /**
     * The **\`console.table()\`** static method displays tabular data as a table.
     *
     * [MDN Reference](https://developer.mozilla.org/docs/Web/API/console/table_static)
     */
    table(tabularData?: any, properties?: string[]): void;
    /**
     * The **\`console.time()\`** static method starts a timer you can use to track how long an operation takes.
     *
     * [MDN Reference](https://developer.mozilla.org/docs/Web/API/console/time_static)
     */
    time(label?: string): void;
    /**
     * The **\`console.timeEnd()\`** static method stops a timer that was previously started by calling console/time_static.
     *
     * [MDN Reference](https://developer.mozilla.org/docs/Web/API/console/timeEnd_static)
     */
    timeEnd(label?: string): void;
    /**
     * The **\`console.timeLog()\`** static method logs the current value of a timer that was previously started by calling console/time_static.
     *
     * [MDN Reference](https://developer.mozilla.org/docs/Web/API/console/timeLog_static)
     */
    timeLog(label?: string, ...data: any[]): void;
    timeStamp(label?: string): void;
    /**
     * The **\`console.trace()\`** static method outputs a stack trace to the console.
     *
     * [MDN Reference](https://developer.mozilla.org/docs/Web/API/console/trace_static)
     */
    trace(...data: any[]): void;
    /**
     * The **\`console.warn()\`** static method outputs a warning message to the console at the 'warning' log level.
     *
     * [MDN Reference](https://developer.mozilla.org/docs/Web/API/console/warn_static)
     */
    warn(...data: any[]): void;
}

var console: Console;

interface Array<T> {
  /**
   * Pipe this array through a transform function.
   *
   * Passes \`this\` to \`pipeFn\` and returns the result, enabling inline
   * functional transforms and method chaining on any array.
   *
   * @param pipeFn A function that receives this array and returns a transformed value
   * @returns The return value of \`pipeFn\`
   *
   * @example
   * // Pipe an array of outputs
   * keys: [$sine('c3'), $sine('e3'), $sine('g3')].pipe(all => $mix(all))
   */
  pipe<U>(this: this, pipeFn: (self: this) => U): U;
}

type NoteNames = "a" | "A" | "b" | "B" | "c" | "C" | "d" | "D" | "e" | "E" | "f" | "F" | "g" | "G"
type Accidental = "" | "#" | "b"
type Note = \`\${NoteNames}\${Accidental}\${number | ''}\`

type HZ = \`\${number}hz\` | \`\${number}Hz\`

/**
 * A chord: a {@link Note}, a colon and a chord symbol, one lane per note,
 * lowest first. The symbol is the one patterns use (see {@link $cycle}).
 * @example $saw("c3:M7")        // four lanes: C3, E3, G3, B3
 * @example $saw("eb:m7^1%")     // first inversion, just intonation
 */
type Chord = \`\${Note}:\${string}\`

/**
 * A scale: a {@link Note} with a scale in parentheses, one lane per note,
 * lowest first, written as in a pattern (see {@link $cycle}).
 * @example $saw("d3(dorian)")       // seven lanes
 * @example $saw("c(0 2 4 7 9)")     // your own scale, in semitones
 */
type Scale = \`\${Note}(\${string})\`

type CaseVariants<T extends string> = 
  | Lowercase<T>
  | Uppercase<T>
  | Capitalize<T>;

type ModeString =
  | "M"
  | "m"
  | CaseVariants<
      | "major" | "maj" | "ionian"
      | "minor" | "min" | "aeolian"
      | "dorian" | "phrygian" | "lydian" | "mixolydian" | "locrian"
      | "harmonic minor" | "harmonicminor" | "har minor"
      | "melodic minor" | "melodicminor" | "mel minor"
      | "pentatonic major" | "pentatonic maj" | "pent major" | "pent maj"
      | "pentatonic minor" | "pentatonic min" | "pent minor" | "pent min"
      | "blues" | "whole tone" | "wholetone" | "chromatic"
    >;

/**
 * A scale or chord spec for a pattern's \`.scale(...)\`: a root note
 * (optional octave, default 4) followed by a scale in \`()\` or a chord in
 * \`[]\`.
 *
 * | Form | Example | Meaning |
 * |------|---------|---------|
 * | \`root(scale)\` | \`"C(major)"\`, \`"D3(dorian)"\` | Named scale |
 * | \`root(intervals)\` | \`"C(0 3 7 10)"\` | Semitones from the root, reduced to one octave |
 * | \`root(tuning)\` | \`"C(just)"\`, \`"A(pythag)"\` | All 12 steps in a non-equal tuning |
 * | \`root(tuning intervals)\` | \`"C(just 0 4 7)"\` | Custom intervals in a non-equal tuning |
 * | \`root[chord]\` | \`"C[maj7]"\`, \`"F#2[m9]"\` | Chord shorthand; extensions keep their register |
 * | \`root[intervals]\` | \`"C[0 7 16]"\` | Custom voicing; intervals keep their register |
 * | \`root[tuning chord]\` | \`"C[just maj]"\` | Chord in a non-equal tuning |
 * | \`root[chord invN]\` | \`"C[maj7 inv1]"\` | Raises the lowest N notes an octave |
 * | \`chromatic\` | \`"chromatic"\` | All 12 semitones from C4 |
 *
 * Scale names: major/maj/ionian/M, minor/min/aeolian/m, dorian, phrygian,
 * lydian, mixolydian, locrian, harmonic minor, melodic minor, pentatonic
 * major/minor, blues, whole tone, chromatic.
 *
 * Chord names (case-sensitive): maj/M/major, min/m/minor, dim/°, aug/+, sus2,
 * sus4/sus, 5, 6, m6, 69, 7/dom7, maj7/M7/Δ/Δ7, m7/min7, mMaj7/mM7, dim7/°7,
 * m7b5/ø/ø7, aug7/7#5/+7, maj7#5, 7sus4, 7sus2, 7b5, 9, maj9, m9, 11, maj11,
 * m11, 13, maj13, m13, 7b9, 7#9, 7#11, add9/add2, madd9, add11/add4.
 */
type ScaleSpec = \`\${string}(\${string})\` | \`\${string}[\${string}]\` | "chromatic";

type OrArray<T> = T | T[];

/**
 * Extracts the element types from a tuple of arrays.
 * Used as the return type of {@link $cartesian} to enable typed destructuring.
 * @example
 * type T = ElementsOf<[number[], string[]]>; // [number, string]
 */
type ElementsOf<T extends unknown[][]> = { [K in keyof T]: T[K] extends (infer E)[] ? E : never };

/**
 * An audio signal value. The fundamental type for all audio connections.
 * 
 * Signals follow the 1V/octave convention where 0V = C4 (~261.63 Hz).
 * 
 * Can be one of:
 * - A **number** (constant voltage)
 * - A **{@link Note}** string like \`"C4"\` or \`"A#3"\`
 * - A **{@link HZ}** string like \`"440hz"\`
 * - A **{@link Chord}** string like \`"c3:M7"\`, one lane per note
 * - A **{@link Scale}** string like \`"d(dorian)"\`, one lane per note
 * - A **{@link Collection}** from another module, one voice per lane
 * 
 * @example $sine("C4")           // Note string
 * @example $sine(2)              // Number
 * @example $sine("440hz")        // Hz string
 * @example $sine($signal('c').add($sine("1hz").amp(0.1)))   // Collection (1Hz vibrato)
 * @see {@link Poly<Signal>} - for multi-lane signals
 * @see {@link Collection} - for module connections
 */
type Signal = number | Note | HZ | Chord | Scale | Collection | CollectionWithRange;

/** A {@link Signal} that holds still: a number, note, frequency, chord or scale. */
type ConstantSignal = number | Note | HZ | Chord | Scale;

/** A {@link Signal} whose range the engine knows, or a constant it carries along. */
type RangedSignal = ConstantSignal | CollectionWithRange;

/**
 * A potentially multi-lane signal for polyphonic patches.
 *
 * Can be:
 * - A single {@link Signal}
 * - An array of {@link Signal}s (creates multiple voices)
 * - A {@link SignalGroup} from \`$g1\`/\`$g2\`/\`$g3\` (cartesian voice expansion)
 *
 * @example chord: $saw(["C3", "E3", "G3"])                    // 3-voice chord
 * @example chord: $saw([...$sine("1hz"), ...$sine("2hz")])   // Spread lanes into voices
 * @see {@link Signal} - for a single signal
 * @see {@link Collection} - for grouping outputs
 */
type Poly<T extends Signal = Signal> = OrArray<T | Collection | CollectionWithRange> | SignalGroup;

/**
 * An axis of an XY scope: signals and collections, or an array mixing
 * them. Every lane is its own trace.
 */
type ScopeAxis = OrArray<Signal>;

/**
 * A param value tagged into a cartesian signal group by {@link $g1}, {@link $g2},
 * or {@link $g3}. Params in different groups multiply within a module — its
 * lane count becomes the product of the group widths — while params in the
 * same group cycle against each other as usual. Untagged params are group 0.
 * Lower-numbered groups vary fastest across the resulting lanes.
 *
 * Opaque: construct only with \`$g1\`/\`$g2\`/\`$g3\`.
 */
interface SignalGroup {
  readonly __kind: "SignalGroup";
  readonly group: 1 | 2 | 3;
}


/**
 * A signal input that sums all lanes to a single mono value. Takes what
 * {@link Poly} takes except a signal group: the module combines all voices
 * into one control signal rather than preserving polyphony.
 *
 * @example $pan($saw("c3"), 0, { spread: 2 })              // Constant spread
 * @example $pan($saw("c3"), 0, { spread: $sine("1hz") })   // Oscillating spread
 * @see {@link Poly} - for polyphonic signals that preserve per-voice data
 * @see {@link Signal} - for single-lane signals
 */
type LaneSum<T extends Signal = Signal> = OrArray<T | Collection | CollectionWithRange>;

/**
 * A phase warp produced by the \`$phase.*\` helpers.
 *
 * Passed to modules that read a phase through a \`PhaseWarp\`-typed param
 * (the \`phase\` param of \`$wavetable\`, \`$enosc\` and \`$pWarp\`) to
 * reshape the phase before it is used.
 *
 * Create one with a \`$phase.*\` helper — do not construct directly.
 *
 * Warps are composable: \`warp.pipe(fn)\` passes this warp to \`fn\` and
 * returns the result, and the optional second argument to any \`$phase.*\`
 * helper feeds this warp's output phase into that warp.
 */
type PhaseWarpDescriptor =
  | { readonly type: "mirror"; readonly amount: Signal }
  | { readonly type: "bend"; readonly amount: Signal }
  | { readonly type: "sync"; readonly ratio: Signal }
  | { readonly type: "fold"; readonly amount: Signal }
  | { readonly type: "pwm"; readonly width: Signal }
  | { readonly type: "pulsar"; readonly amount: Signal }
  | { readonly type: "crush"; readonly amount: Signal }
  | { readonly type: "feedback"; readonly amount: Signal }
  | { readonly type: "identity" }
  | { readonly type: "pipe"; readonly first: PhaseWarp; readonly second: PhaseWarp };

type PhaseWarp = PhaseWarpDescriptor & {
  /** Pass this warp to \`pipeFn\` and return the result. Mirrors \`Collection.pipe\`. */
  pipe<T>(pipeFn: (self: PhaseWarp) => T): T;
};

/**
 * A scale for \`$enosc\`'s \`scale\` param. Create one with a
 * \`$enosc.scale.*\` helper — do not construct directly.
 */
type EnoscScale =
  | { readonly type: Exclude<keyof typeof $enosc.scale, "custom"> }
  | { readonly type: "custom"; readonly notes: readonly number[]; readonly repeatsOctave: boolean };

/**
 * A buffer output reference — returned by \`$buffer()\`, passed to readers
 * (like \`$bufRead\`, \`$delayRead\`) as their \`buffer\` param.
 */
type BufferOutputRef = {
  readonly type: "buffer_ref";
  readonly module: string;
  readonly port: string;
  readonly lanes: number;
  readonly frameCount: number;
};

type DropFirst<T extends unknown[]> = T extends [unknown, ...infer R] ? R : never;

/**
 * The \`$cycle\` sequencer a pattern plays through, as the pattern has it.
 * What a member gives back (a lane, a slice, what \`.scope()\` returns)
 * is that sequencer's signal, not a pattern.
 */
type PatternModule = { [K in keyof CycleOutputs]: CycleOutputs[K] };

/**
 * A mini-notation pattern, as \`$\`, \`$arrange\` and the pattern
 * methods return it. A pattern is the \`$cycle\` sequencer that
 * plays it: use it wherever a module goes, read its outputs (\`_gate\`,
 * \`_trig\`) and chain any collection method. Every such use of one pattern
 * shares one sequencer. \`.fast\`, \`.slow\`, \`.struct\`, \`.beat\`,
 * \`.glide\`, \`.playhead\` and \`.ribbon\` each give a new pattern, a
 * {@link ScalePattern} when called on one; \`$cycle(pattern, …)\` and
 * \`.cycle(…)\` play it through a sequencer of its own with other settings.
 */
interface Pattern extends PatternModule {
  /**
   * Speed this pattern up by \`factor\`, mirroring Strudel's \`fast\`. The factor
   * is a constant (\`2\`) or a mini-notation number pattern (\`"2 4"\` → ×2 for
   * the first half of the cycle, ×4 for the second). A non-positive factor
   * (\`fast(0)\` or negative) is silence. Chains and nests with the other
   * pattern builders.
   */
  fast(factor: number | string): this;
  /**
   * Slow this pattern down by \`factor\` — the time-inverse of \`fast\`.
   * \`slow(n)\` equals \`fast(1 / n)\`. The factor may be a constant or a
   * mini-notation number pattern.
   */
  slow(factor: number | string): this;
  /**
   * Impose a rhythmic structure on this pattern, mirroring Strudel's
   * \`struct\`: event timing comes from the boolean pattern, and this
   * pattern's value is sampled at each onset slot.
   *
   * | Token | Meaning |
   * |-------|---------|
   * | \`x\` (or any nonzero number) | Onset — sample this pattern here |
   * | \`~\`, \`-\`, or \`0\` | Rest (silent slot) |
   *
   * The boolean pattern supports the full mini-notation grammar —
   * \`"x ~ <x ~>"\` alternates its third slot per cycle, \`"x(3,8)"\` is a
   * euclidean structure, and so on.
   *
   * \`\`\`js
   * melody: $\`c4 e4\`.struct("x ~ x x").saw()
   * \`\`\`
   */
  struct(boolPattern: string): this;
  /**
   * Place this pattern's value at chosen beats of a divided cycle,
   * mirroring Strudel's \`beat\`. \`t\` lists beat indices — a comma stack
   * (\`"0,7,10"\`) plays one onset per index — and \`div\` is the number of
   * beats per cycle. Each onset lasts \`1/div\` of a cycle; \`t\` wraps modulo
   * \`div\` and may be fractional. A slot not fully inside the cycle is
   * silent, as in Strudel: negative beats produce no onset, as does a
   * fractional beat within \`1\` of the cycle end. Both arguments accept a
   * constant number or a mini-notation number pattern (\`"<16 8>"\` changes
   * the grid per cycle).
   *
   * \`\`\`js
   * melody: $\`c2\`.beat("0,7,10", 16).saw()
   * \`\`\`
   */
  beat(t: number | string, div: number | string): this;
  /**
   * Give each note a glide (portamento) of \`g\` cycles. \`$cycle\` ramps
   * its CV from the previous pitch, starting \`g\` cycles before the
   * note's onset and arriving on it. \`g\` is sampled at each onset;
   * \`0\` jumps. It is a constant or a mini-notation number pattern. With
   * several voices, each gliding note takes the nearest-pitched voice that is
   * free at its onset.
   *
   * \`curve\` shapes the ramp (default \`'linear'\`) and \`mode\` picks
   * which end it eases: \`'in'\` (default), \`'out'\` or \`'inOut'\`.
   * Every curve stays between the two pitches.
   *
   * \`\`\`js
   * melody: $sine($\`c4 e4 g4\`.glide("0 0.1 0.25"))
   * melody2: $sine($\`c3 g3\`.glide(0.25, 'expo', 'out'))
   * \`\`\`
   */
  glide(g: number | string, curve?: GlideCurve, mode?: GlideMode): this;
  /**
   * Read this pattern's atoms as **0-indexed degrees** of \`scale\`, making a
   * {@link ScalePattern}: \`0\` is the scale's root, \`1\` its second tone,
   * \`2\` its third, and so on. Negative degrees move downward, and degrees
   * past the scale's length wrap into the octaves above and below. Note and
   * Hz atoms are errors under a scale.
   *
   * On a scale pattern, \`.scale\` changes the scale of every note. On an
   * arrangement it scales every section, plain or scaled.
   *
   * \`\`\`js
   * melody: $\`0 2 4 7\`.scale("c(major)").saw()       // C-major arpeggio
   * melody2: $\`-1 0 2 4\`.scale("a3(min)").saw()       // negative degrees wrap below the root
   * melody3: $\`0 1 2 3 4\`.scale("c(0 2 4 7 9)").saw() // custom intervals (pentatonic)
   * melody4: $\`0 4 7\`.scale("c(just)").saw()          // just intonation
   * melody5: $\`0 2 4 4+ 5\`.scale("c(major)").saw()   // 4+ is G#, a passing note outside the scale
   * \`\`\`
   *
   * ### Atoms
   *
   * | Form | Meaning | Example |
   * |------|---------|---------|
   * | Bare integer | Scale degree (0-indexed) | \`0\`, \`2\`, \`-1\` |
   * | Integer then \`+\`s or \`-\`s | That degree, raised or lowered a semitone per mark, so it can leave the scale | \`2+\`, \`4--\`, \`-1+\` |
   * | \`~\` or \`-\` | Rest (gate low, no pitch change) | \`'0 ~ 2 ~'\`, \`'0 - 2 -'\` |
   *
   * ### Scale strings
   *
   * | Form | Example | Meaning |
   * |------|---------|---------|
   * | \`tonic(name)\` | \`"c(major)"\`, \`"d#(min)"\`, \`"f(dorian)"\` | Named scale type rooted at the tonic |
   * | \`tonic<octave>(name)\` | \`"c3(major)"\`, \`"D#4(min)"\` | Same with an explicit octave (default 4 → root = C4 = 0 V) |
   * | \`tonic(custom intervals)\` | \`"c(0 2 4 5 7 9 11)"\` | Custom semitone offsets from the tonic |
   * | \`tonic(just)\` / \`tonic(pythagorean)\` | \`"c(just)"\`, \`"a(pythag)"\` | Non-equal 12-tone tunings |
   * | \`tonic[chord]\` | \`"c[maj7]"\`, \`"a3[m9]"\` | Chord tones; degrees step through the chord |
   * | \`tonic[chord invN]\` | \`"c[maj inv1]"\` | Inverted chord; degree 0 is the lowest note |
   * | \`chromatic\` | \`"chromatic"\` | All 12 semitones, 12-TET |
   *
   * A semitone mark steps to the neighbouring note of the scale's 12-tone
   * tuning, so in \`"c(just)"\` it lands on a just-tuned note.
   *
   * Degrees wrap by an octave for scales. A chord wraps by its span rounded up
   * to whole octaves, so \`"c[9]"\` repeats every two octaves. See
   * {@link ScaleSpec} for the full list of scale and chord names.
   *
   * @param scale scale string, e.g. "c(major)", "D#3(min)", "a(just)"
   */
  scale(scale: string): ScalePattern;
  /**
   * Play this pattern from \`playhead\`, a position in cycles, in place of
   * the global clock: every \`$cycle\` that plays it reads the pattern there,
   * unless given a \`playhead\` of its own. Kept through later pattern
   * methods; a later \`.playhead\` replaces it.
   *
   * \`\`\`js
   * melody: $\`c4 e4 g4 b4\`.playhead($clock._playhead.mul(0.5)).saw() // half speed
   * \`\`\`
   */
  playhead(playhead: LaneSum<Signal>): this;
  /**
   * Loop the stretch of this pattern \`length\` cycles long from cycle
   * \`offset\`: every \`$cycle\` that plays it loops there, unless given a
   * \`ribbon\` of its own. Kept through later pattern methods; a later
   * \`.ribbon\` replaces it.
   *
   * \`\`\`js
   * melody: $\`<c4 e4 g4 b4>\`.ribbon(1, 2).saw() // loops e4 g4
   * \`\`\`
   */
  ribbon(offset: number, length: number): this;
  /**
   * Shorthand for \`$cycle(pattern, ...rest)\`: plays this pattern through
   * a \`$cycle\` of its own, with the arguments \`$cycle\` takes after its
   * \`pattern\`.
   *
   * \`\`\`js
   * melody: $\`c4 e4 g4\`.cycle({ ribbon: [0, 2] }).saw()
   * \`\`\`
   */
  cycle(...rest: DropFirst<Parameters<typeof $cycle>>): ReturnType<typeof $cycle>;
  /**
   * This pattern's \`$cycle\` sequencer followed by \`other\`'s, as one
   * sequencer signal with each of their outputs joined lane by lane.
   *
   * \`\`\`js
   * const seq = $\`c4 e4\`.concat($\`g3 b3\`)
   * melody: $saw(seq).amp($perc(seq))
   * \`\`\`
   */
  concat(other: Pattern | CycleOutputs): CycleOutputs;
}

/**
 * One Strudel-style alignment for \`.trans\`.
 */
type AlignmentMode = ${literalUnion(ALIGNMENT_MODES)};

/**
 * \`.trans\` on a {@link ScalePattern}: callable, with a method for each
 * alignment. Called bare, it is \`.in\`.
 */
type Transpose = ((rhs: string) => ScalePattern) & {
  readonly [M in AlignmentMode]: (rhs: string) => ScalePattern;
};

/**
 * A scale-degree pattern, made by \`.scale(...)\`: a {@link Pattern} that
 * \`.trans\` also transposes. It stays one through every pattern method,
 * and an arrangement whose every section is one is one too.
 */
interface ScalePattern extends Pattern {
  /**
   * \`.trans(...)\` transposes a scale pattern by another scale-degree
   * mini-notation string: the two patterns' degrees are added, so negative
   * degrees transpose down, and each note keeps its own scale. A note's
   * \`+\`/\`-\` semitone marks stay with it, and marks on a \`.trans\` degree
   * (\`.trans("0+")\`) raise or lower every note that degree moves.
   * \`.add(...)\` and \`.sub(...)\` are the collection's: they add to and
   * subtract from the sequencer's output signal, in volts.
   * Bare \`.trans(x)\` defaults to \`.trans.in(x)\`; explicit
   * mode methods cover all seven Strudel alignments:
   *
   * | Mode | Behaviour |
   * |------|-----------|
   * | \`.in\` (default) | Left pattern's onsets drive timing; right is sampled at each left event. |
   * | \`.out\` | Right pattern's onsets drive timing; left is sampled at each right event. |
   * | \`.mix\` | Output events at every intersection of left + right onsets. |
   * | \`.squeeze\` | Each left event nests a full cycle of the right pattern. |
   * | \`.squeezeout\` | Each right event nests a full cycle of the left pattern. |
   * | \`.reset\` | Right pattern retriggers the left, aligned to cycle position. |
   * | \`.restart\` | Right pattern retriggers the left, aligned to cycle 0. |
   *
   * \`\`\`js
   * melody: $\`0 1 2\`.scale("c(maj)").trans("0 2").saw()   // .trans defaults to .trans.in
   * melody2: $\`0 1 2\`.scale("d#(min)").trans.in("10 20").saw()
   * melody3: $\`0 1 2\`.scale("c(maj)").trans("<0 3>").fast(2).trans.squeeze("0 -1").saw()
   * \`\`\`
   */
  trans: Transpose;
}

/** Shape of a glide ramp. See \`$(...).glide\`. */
type GlideCurve = ${literalUnion(GLIDE_CURVES)};

/** Which end of a glide ramp its curve eases. See \`$(...).glide\`. */
type GlideMode = ${literalUnion(GLIDE_MODES)};

/**
 * What \`$wavMeta\` reads from a WAV file. Times are in seconds.
 */
type WavMeta = {
  readonly path: string;
  readonly lanes: number;
  readonly sampleRate: number;
  readonly frameCount: number;
  readonly duration: number;
  readonly bitDepth: number;
  /** Root note of the sample, in V/Oct. */
  readonly pitch?: number;
  readonly playback?: 'one-shot' | 'loop';
  readonly bpm?: number;
  readonly beats?: number;
  readonly timeSignature?: {
    readonly num: number;
    readonly den: number;
  };
  /** Number of bars the sample spans, computed from BPM and time signature. E.g. an exact 2-bar loop is \`2.0\`; a 2.64-bar buffer is \`2.64\`. Absent when no BPM could be derived. */
  readonly barCount?: number;
  readonly loops: ReadonlyArray<{
    readonly type: 'forward' | 'pingpong' | 'backward';
    readonly start: number;
    readonly end: number;
  }>;
  readonly cuePoints: ReadonlyArray<{
    readonly position: number;
    readonly label: string;
  }>;
};

/**
 * A send to a bus, named by its track's name: the name alone sends at full
 * level, a \`[name, level]\` pair at a linear \`level\` (5 = full, 0 = none).
 */
type Send = BusTap | [bus: BusTap, level: SendLevel];

/**
 * A send's level: any signal but a track's name, which in a send is always
 * another bus to send to. Wrap a track's name to use it as a level:
 * \`[bus, $c(name)]\`.
 */
type SendLevel = Poly<Signal> & { readonly active?: never };

/**
 * Options of {@link BaseCollection.track}: how a track mixes into the
 * mixer. Its name, mute, solo and channels come from its label.
 */
interface TrackOptions {
${renderTrackOptionsMembers(TRACK_OPTIONS)}
}

/**
 * Options of {@link BusSignal.track}: those of {@link TrackOptions} but
 * \`sends\`, since a bus cannot send to another bus.
 */
interface BusTrackOptions {
${renderTrackOptionsMembers(BUS_TRACK_OPTIONS)}
}

/** Options of a scope, from \`.scope()\`. */
interface ScopeConfig {
  /** Time window in milliseconds (default 500) */
  msPerFrame?: number;
  /** Trigger threshold in volts */
  triggerThreshold?: number;
  /** Whether the scope waits to render until the buffer fills (default true). Only applies with a triggerThreshold */
  triggerWaitToRender?: boolean;
  /** Voltage range for display as [min, max] (default [-5, 5]) */
  range?: [number, number];
  /** Draw each lane in its own scope instead of overlaying them: 'vertical' stacks them top to bottom, 'horizontal' fits them side by side */
  split?: 'vertical' | 'horizontal';
}

/**
 * A signal or bus with how it mixes into the mixer, from \`.track()\`. A
 * track is heard once it is labelled: \`lead: $saw('c3').track({ pan: -2 })\`.
 * Only \`.scope()\` chains on it.
 */
class Track<S = Collection> {
  private constructor();
  /** Sets a track apart from what it was made from. */
  #source: S;

  /**
   * Scope the track's mixdown: one lane, mono or stereo, after its pan,
   * spread and gain.
   * @param config Scope configuration options
   * @example lead: $saw(['c3', 'e3']).track({ pan: -2 }).scope()
   */
  scope(config?: ScopeConfig): this;
}

/**
 * What a track label's name reads as in the rest of the patch: the track's
 * final signal, which \`sends\` also takes when the track is a bus, and
 * label suffixes name when it is a group.
 * @internal
 */
declare function __trackBinding<T>(value: T): 0 extends 1 & T ? TrackTap : [T] extends [BusRooted | Track<BusRooted>] ? BusTap : [T] extends [GroupRooted | Track<GroupRooted>] ? GroupTap : TrackTap;

/**
 * Every signal in a patch: an ordered set of lanes, one voice each, with
 * chainable DSP methods. Indexing and iteration give one-lane collections
 * of the same type, so \`osc[0]\` is still a signal with every method.
 */
interface BaseCollection<Plain = Collection, Ranged = CollectionWithRange> extends ModuleChain<Plain, Ranged> {}
class BaseCollection<Plain = Collection, Ranged = CollectionWithRange> {
  /** Number of lanes in the collection */
  readonly length: number;
  /** Lane \`index\` as a one-lane collection */
  readonly [index: number]: this;
  [Symbol.iterator](): Iterator<this>;

  /**
   * A collection of the same type holding the lanes from \`start\` up to but
   * not including \`end\`. Negative indices count from the end, as with
   * \`Array.prototype.slice\`.
   * @param start First index to include (default 0)
   * @param end Index to stop before (default: the collection's length)
   * @example chord: $saw(['c3', 'e3', 'g3', 'b3']).slice(1, 3)
   */
  slice(start?: number, end?: number): this;

  /**
   * Split into an array of consecutive collections of the same type, each
   * \`size\` lanes long except the last, which holds the remainder.
   *
   * \`\`\`js
   * const [low, high] = $saw(['c3', 'e3', 'g3', 'b3']).chunk(2)
   * sound: low.track({ pan: -5 })
   * sound2: high.track({ pan: 5 })
   * \`\`\`
   * @param size Lanes per chunk (a positive integer)
   */
  chunk(size: number): this[];

  /**
   * This collection's lanes followed by those of \`other\`, a collection of
   * the same type. A module with several outputs joins output by output: the
   * result has every output by name and still stands for whichever fits the
   * input it is plugged into. It also takes a plain signal, whose lanes
   * follow those of every output.
   *
   * \`\`\`js
   * const oscs = $enosc('c3').concat($enosc('g3'))
   * keys: oscs
   * keys2$2m: oscs._root.lpf('400hz')
   * \`\`\`
   * @param other A collection of the same type
   */
  concat(other: this): this;

  /**
   * Add scope visualization for all lanes, overlaid on one scope unless \`split\` is set.
   * The scope appears as an overlay in the editor.
   * @param config Scope configuration options
   * @param config.msPerFrame Time window in milliseconds (default 500)
   * @param config.triggerThreshold Trigger threshold in volts (optional)
   * @param config.triggerWaitToRender Whether the scope should wait to render until the buffer fills (default true). Only applicable if triggerThreshold is set.
   * @param config.range Voltage range for display as [min, max] tuple (default [-5, 5])
   * @param config.split Draw each lane in its own scope instead of overlaying them: 'vertical' stacks them top to bottom, 'horizontal' fits them side by side
   */
  scope(config?: ScopeConfig): this;

  /**
   * How this signal mixes into its track. A track is heard once its
   * statement is labelled: the label names it and sets its mute, solo and
   * channels (\`_name\` mutes, \`S_name\` solos, \`S__name\` does both, \`name$2\` plays on channels
   * 2 and 3, \`name$2m\` in mono on channel 2, \`name$drums\` into the group
   * track \`drums\`). The name then reads as the
   * track's final signal in the rest of the patch.
   *
   * Options:
${renderTrackOptionsJsDocList(TRACK_OPTIONS)}
   * @param options Track options ({@link TrackOptions})
   * @example lead: $sine('c4').track({ gain: 2.5, pan: -2 })
   * @example
   * verb: $bus().plate()
   * keys: $saw('c3').track({ sends: [verb, 2] })
   */
  track(options?: TrackOptions): Track<this>;

  /**
   * Remap this signal from an explicit input range to a new output range.
   * Creates a $remap module internally.
   * @param outMin New minimum as {@link Poly<Signal>}
   * @param outMax New maximum as {@link Poly<Signal>}
   * @param inMin Input minimum as {@link Poly<Signal>}
   * @param inMax Input maximum as {@link Poly<Signal>}
   * @returns A {@link CollectionWithRange} carrying the remapped signal
   * @example $sine('c4').range(0, 1, -5, 5)
   * @see {@link CollectionWithRange.range} - for automatic input range
   */
  range(outMin: Poly<Signal>, outMax: Poly<Signal>, inMin: Poly<Signal>, inMax: Poly<Signal>): CollectionWithRange;

  /**
   * Pipe this signal through a transform function.
   *
   * Passes \`this\` to \`pipeFn\` and returns the result, enabling inline
   * functional transforms and reusable signal-processing helpers.
   *
   * @param pipeFn A function that receives this signal and returns a transformed value
   * @returns The return value of \`pipeFn\`
   *
   * @example
   * // Inline transform
   * $sine('a').pipe(s => s.amp(0.5).add(1))
   *
   * @example
   * // Reusable helper
   * const tremolo = (c) => c.amp($sine('10hz').range(4, 5))
   * tone: $saw('c').pipe(tremolo)
   */
  pipe<T>(pipeFn: (self: this) => T): T;
  /**
   * Pipe this signal through a transform for each element of an array.
   * Returns a {@link Collection} of every result's lanes.
   *
   * @param pipeFn A function that receives this signal and one element from the array
   * @param array An array whose elements are passed to \`pipeFn\` one by one
   * @returns A {@link Collection} with the lanes of every result
   *
   * @example
   * // 6 lanes
   * chord: $sine(['C4', 'E4', 'G4']).pipe(
   *   (s, cut) => $lpf(s, cut),
   *   ['440hz', '880hz'],
   * )
   */
  pipe<T extends Poly<Signal>, E>(
    pipeFn: (self: this, item: E) => T,
    array: E[]
  ): Collection;

  /**
   * Pipe this signal through a transform, then mix the original and transformed
   * signals together using a \\$mix module.
   *
   * @param pipeFn A function that receives this signal and returns a signal to mix with the original
   * @param mix Optional crossfade as {@link Poly<Signal>}. 0 for only original, 5 for only transformed. Default is 2.5 for equal mix.
   * @returns A Collection from the \\$mix output
   *
   * @example
   * // Mix original with a filtered version
   * lead: $saw('c4').pipeMix(s => $lpf(s, '1000hz'))
   *
   * @example
   * // Mix with custom balance
   * lead: $saw('c4').pipeMix(s => $lpf(s, '1000hz'), 1.0)
   */
  pipeMix(pipeFn: (self: this) => Collection | CollectionWithRange, mix?: Poly<Signal> ): Plain;

  /**
   * Crossfade the signal this one was chained from against it. Follows a
   * chained module: \`sig.lpf('800hz').wet(2.5)\` mixes \`sig\` with its
   * filtered copy. After a stereo effect such as \`.plate()\`, which mixes
   * every voice into one stereo lane, the dry voices are mixed into one too.
   *
   * @param mix Dry/wet crossfade as {@link Poly<Signal>}. 0 = dry only, 5 = wet only. Default is 2.5 for an equal mix.
   * @returns A Collection from the \\$mix output
   *
   * @example keys: $saw('c3').plate().wet(1.5)
   */
  wet(mix?: Poly<Signal>): Plain;
}

/**
 * A signal with chainable DSP methods: what every module returns, one
 * lane per voice.
 * 
 * Group signals with the $c() helper function. Supports iteration, indexing,
 * and spreading, each giving one-lane collections.
 * 
 * @example
 * const voices = $c($sine('c3'), $sine('e3'), $sine('g3'))
 * keys: voices.amp(0.5)          // Apply amplitude to all
 * for (const v of voices) v.scope()    // Iterate
 * const arr = [...voices]              // Spread to array
 * voices[0]                            // Index access
 *
 * @see {@link CollectionWithRange} - for signals with a known range
 * @see {@link $c} - helper to create Collection
 */
class Collection extends BaseCollection {
  protected constructor();
}

/**
 * A signal whose range the engine knows: a {@link CollectionWithRange}, or
 * a {@link RangedBusSignal} when rooted at a bus.
 */
class RangedCollection<Plain, Ranged extends Plain> extends BaseCollection<Plain, Ranged> {
  /** Sets a ranged collection apart from one whose range is unknown. */
  #ranged: true;
  protected constructor();

  /**
   * Remap from the signal's range, or from \`inMin\`–\`inMax\` when given, to a
   * new range.
   * @param outMin New minimum as {@link Poly<Signal>}
   * @param outMax New maximum as {@link Poly<Signal>}
   * @param inMin Input value that maps to \`outMin\` (default: the start of the signal's range)
   * @param inMax Input value that maps to \`outMax\` (default: the end of the signal's range)
   * @returns A {@link CollectionWithRange} carrying the remapped signal
   * @example $sine("1hz").range("C3", "C5")
   * @example $sine("1hz").range(0, 5).range(5, 0)  // flipped
   */
  override range(outMin: Poly<Signal>, outMax: Poly<Signal>, inMin?: Poly<Signal>, inMax?: Poly<Signal>): CollectionWithRange;
}

/**
 * A {@link Collection} whose range the engine knows, as LFOs, envelopes,
 * controls and other modulation sources return, and as chains that keep a
 * range (add, mul, round, erratic…) pass on. The range() method maps from it.
 * 
 * @example
 * const lfo = $sine('2hz')             // Outputs -5 to +5
 * lfo.range(200, 2000)                 // Remap to 200-2000
 * $adsr($clock._beatTrigger, { attack: 0.1 }).range(0, 1)
 * tone: $r($sine('1hz'), $sine('2hz')).range(0, 5)  // Each maps from its own range
 *
 * @see {@link Collection} - for signals without a known range
 * @see {@link $r} - helper to create CollectionWithRange
 */
class CollectionWithRange extends RangedCollection<Collection, CollectionWithRange> {
  protected constructor();
}

/**
 * Placeholder lanes for signals assigned later, from {@link $deferred}:
 * use it as a signal now and call .set() before the end of the script, for
 * feedback loops and forward references.
 */
class DeferredCollection extends BaseCollection {
  protected constructor();

  /**
   * Set the signals for all lanes, cycling a narrower argument across
   * them. Bare {@link Signal} literals (numbers, note/Hz strings) are lifted
   * into $signal modules, matching $c — a string is one signal, not spread
   * into characters.
   * @param polySignal A {@link Poly<Signal>} (single signal or array) to distribute across the lanes
   */
  set(polySignal: Poly<Signal>): void;
}

/**
 * A track's final signal, as its label's name reads and as {@link $trackTap}
 * returns it. As a {@link Collection} it is the track's audio after its pan,
 * gain and mute/solo gate: one lane, stereo for a stereo track.
 */
class TrackTap extends Collection {
  protected constructor();
${renderTrackTapMembers()}
}


${renderFunctions(GLOBALS).join('\n')}

${generateVideoGlobals()}


/**
 * Groove template: per-step timing offsets and velocities over a grid. Step
 * offsets are in grid steps (positive = late), and the template repeats every
 * timing.length steps.
 */
interface GrooveTemplate {
  /** Per-step timing offset in grid steps (positive = late). */
  timing: number[];
  /** Per-step velocity (0-1), one per timing entry; every step is 1 when omitted. */
  velocity?: number[];
  /** Grid steps per cycle (default 16 = sixteenth notes in a 4/4 bar). */
  grid?: number;
  /** Timing strength: scales every offset (0 = straight, default 1 = as written). */
  amount?: number;
  /** Velocity strength (0 = every step full, default 1 = as written). */
  velocityAmount?: number;
  /** Largest random timing offset per step, in grid steps (default 0). */
  random?: number;
  /** Seed for the per-step random offsets. */
  seed?: number;
}

/**
 * A reference to a group of controls in the Control panel, created by
 * {@link $cGroup}. Pass it as a control's \`group\` argument (or another
 * group's \`group\` param) to place that control inside the group.
 */
interface ControlGroup {
    /** The group's display label. */
    readonly label: string;
}

/** Options for {@link $cGroup}. */
interface ControlGroupParams {
    /** Whether the group starts collapsed (must be a true/false literal). The Control panel rewrites it when the header is clicked. */
    collapsed?: boolean;
    /** The {@link ControlGroup} to nest this group in (a $cGroup call or a const bound to one). */
    group?: ControlGroup;
}


/** @internal */
declare const __busRoot: unique symbol;

/**
 * The signal of a send-return bus, from {@link $bus}: one stereo lane, the
 * mix of everything sent to it. Chain methods keep it a bus signal, so
 * \`$bus().plate().wet(2)\` is one too, and so is any output of a chained
 * module with several (\`$bus().lpf('c5')._24db\`). The track labelling it
 * is the bus's track, whose name tracks send to.
 */
class BusSignal extends BaseCollection<BusSignal, RangedBusSignal> {
  private constructor();
  /** Sets a bus signal apart from any other. */
  readonly [__busRoot]: true;

  /**
   * How the bus mixes into its track.
   *
   * Options:
${renderTrackOptionsJsDocList(BUS_TRACK_OPTIONS)}
   * @param options Track options ({@link BusTrackOptions})
   * @example
   * verb: $bus().plate().track({ gain: 3 })
   * keys: $saw('c3').track({ sends: [verb, 2] })
   */
  override track(options?: BusTrackOptions): Track<this>;

  /**
   * This bus signal's lane followed by those of \`other\`, still rooted at
   * the bus.
   * @param other Any signal
   */
  override concat(other: Collection | CollectionWithRange | BusRooted): this;
}

/**
 * A {@link BusSignal} whose range the engine knows, as chain methods such as
 * \`.reverb()\` give on a bus signal.
 */
class RangedBusSignal extends RangedCollection<BusSignal, RangedBusSignal> {
  private constructor();
  /** Sets a bus signal apart from any other. */
  readonly [__busRoot]: true;

  /**
   * How the bus mixes into its track.
   * @param options Track options ({@link BusTrackOptions})
   */
  override track(options?: BusTrackOptions): Track<this>;

  /**
   * This bus signal's lane followed by those of \`other\`, still rooted at
   * the bus.
   * @param other Any signal
   */
  override concat(other: Collection | CollectionWithRange | BusRooted): this;
}

/** A signal rooted at a bus: \`$bus()\` or methods chained on it. */
type BusRooted = BusSignal | RangedBusSignal;

/**
 * A bus track, as its label's name reads: what the \`sends\` option of other
 * tracks takes, with the bus's mixer controls. It has no audio: a bus's audio
 * is processed only on its own track, by chaining onto its \`$bus()\`.
 */
class BusTap {
  private constructor();
  /** Sets a bus track's name apart from any other track's. */
  #busTrack: true;
  /** 5V while the bus is audible, 0V while it is muted. */
  readonly active: CollectionWithRange;
  /**
   * The bus's gain (5 = unity), as the mixer's fader shows it.
   * Ranged 0 to 10, or the gain signal's own range when it declares one.
   */
  readonly level: CollectionWithRange;
  /**
   * The bus's pan (-5 left to +5 right). Ranged -5 to 5, or the pan signal's
   * own range when it declares one.
   */
  readonly panning: CollectionWithRange;
}

/** @internal */
declare const __groupRoot: unique symbol;

/**
 * The signal of a group, from {@link $group}: one stereo lane, the sum of
 * the tracks that play into it. Chain methods keep it a group signal,
 * whichever output of a chained module is read. The track labelling it is
 * the group's track, whose name other tracks' labels end in.
 */
class GroupSignal extends BaseCollection<GroupSignal, RangedGroupSignal> {
  private constructor();
  /** Sets a group signal apart from any other. */
  readonly [__groupRoot]: true;

  /**
   * This group signal's lane followed by those of \`other\`, still rooted
   * at the group.
   * @param other Any signal
   */
  override concat(other: Collection | CollectionWithRange | GroupRooted): this;
}

/**
 * A {@link GroupSignal} whose range the engine knows, as chain methods such
 * as \`.reverb()\` give on a group signal.
 */
class RangedGroupSignal extends RangedCollection<GroupSignal, RangedGroupSignal> {
  private constructor();
  /** Sets a group signal apart from any other. */
  readonly [__groupRoot]: true;

  /**
   * This group signal's lane followed by those of \`other\`, still rooted
   * at the group.
   * @param other Any signal
   */
  override concat(other: Collection | CollectionWithRange | GroupRooted): this;
}

/** A signal rooted at a group: \`$group()\` or methods chained on it. */
type GroupRooted = GroupSignal | RangedGroupSignal;

/**
 * A group track's final signal, as its label's name reads: a
 * {@link TrackTap} whose name other tracks' labels end in to play into it.
 */
class GroupTap extends TrackTap {
  private constructor();
  /** Sets a group track's name apart from any other track's. */
  #groupTrack: true;
}

/**
 * Phase warps for modules that accept a {@link PhaseWarp} (the \`phase\`
 * param of \`$wavetable\`, \`$enosc\` and \`$pWarp\`).
 *
 * Each helper returns a {@link PhaseWarp} whose inner signal-valued field
 * accepts a constant, a module output, or any other {@link Signal}.
 *
 * Warps compose via the optional second argument. \`.pipe(fn)\` passes
 * the warp to \`fn\` and returns the result — same API as \`Collection.pipe\`.
 *
 * \`pulsar\`, \`crush\` and \`feedback\` are the 4ms Ensemble Oscillator's
 * twist modes. Every amount is a 0–5 knob (bend: -5..5). A warp can hold at
 * most 8 stages, 4 of them \`feedback\`.
 *
 * @example
 * // Compose two warps (mirror feeds into bend):
 * $phase.mirror(2.5, $phase.bend(1.5))
 *
 * // Compose three warps left-to-right:
 * $phase.mirror(2.5, $phase.bend(1.5, $phase.fold(1)))
 *
 * // Generic function application via .pipe:
 * const addBend = (t) => $phase.bend(1.5, t)
 * $phase.mirror(2.5).pipe(addBend)
 */
declare const $phase: {
    /** Reflect the phase around its midpoint by \`amount\` (0–5). */
    mirror(amount: Poly<Signal>, next?: PhaseWarp): PhaseWarp;
    /** Bend the phase curve by \`amount\` (-5..5; 0 is linear, negative bends the other way). */
    bend(amount: Poly<Signal>, next?: PhaseWarp): PhaseWarp;
    /** Hard-sync: run the phase 1× to 16× faster, restarting each cycle (0–5). */
    sync(ratio: Poly<Signal>, next?: PhaseWarp): PhaseWarp;
    /** Fold the phase back on itself by \`amount\` (0–5). */
    fold(amount: Poly<Signal>, next?: PhaseWarp): PhaseWarp;
    /** Pulse-width modulation warp with duty cycle \`width\` (0–5; 2.5 is even). */
    pwm(width: Poly<Signal>, next?: PhaseWarp): PhaseWarp;
    /**
     * Pulsar: play each cycle up to 64× faster, then rest (0–5). Antialiased
     * when the reading module knows its pitch.
     */
    pulsar(amount: Poly<Signal>, next?: PhaseWarp): PhaseWarp;
    /** Crush: XOR the phase with scaled copies of itself (0–5). */
    crush(amount: Poly<Signal>, next?: PhaseWarp): PhaseWarp;
    /**
     * Feedback: offset the phase by a lowpassed sine of its own output (0–5).
     * Each stage keeps its own state per lane or voice.
     */
    feedback(amount: Poly<Signal>, next?: PhaseWarp): PhaseWarp;
};

/**
 * A video signal: a color with an alpha at every pixel, each channel in the
 * range 0..1. Generators such as \`$v.osc\`, \`$v.noise\` and \`$v.shape\`
 * make gray signals; \`$v.hsv\`, \`$v.solid\`, \`$v.image\` and others
 * make colors. Display one with \`$v.out\`. Where a module wants a number,
 * a signal gives its brightness.
 */
interface VideoSignal {
    readonly __videoSignal: true;
    /** The red channel as a gray signal. */
    readonly r: VideoSignal;
    /** The green channel as a gray signal. */
    readonly g: VideoSignal;
    /** The blue channel as a gray signal. */
    readonly b: VideoSignal;
    /** The alpha as a gray signal. */
    readonly a: VideoSignal;
${generateVideoChainMembers()}
}

/**
 * A video file's picture from \`$v.video\`: a {@link VideoSignal} that also
 * has the file's sound as \`audio\`, an audio signal. Chaining from it, such
 * as \`.hsv()\`, gives a plain {@link VideoSignal}; \`out\` and
 * \`preview\` return the file itself.
 */
interface VideoFile extends VideoSignal {
    /** The sound of the file, playing at its speed between its loop points. */
    readonly audio: Collection;
}

/**
 * A constant, a {@link VideoSignal}, or an audio signal whose live value drives
 * the input: a \`$slider\` / \`$btn\` / \`$toggleBtn\`, or any module output
 * such as an LFO or envelope, with several lanes summed to one. A
 * pattern's values are used as they are, so \`\` $\`0 0.5 1\` \`\` steps from 0
 * to 1.
 */
type VideoValue = number | VideoSignal | Collection | CollectionWithRange;

/**
 * A frame store from \`$v.buffer\`: \`write\` a signal into it, and \`read\`
 * what it held on the previous frame.
 */
interface VideoBuffer {
    /**
     * The previous frame of the buffer, resampled through a zoom, rotation,
     * shift and edge mode, as for \`$v.feedback\`.
     */
    read(config?: {
        /** Magnification about the center; above 1 zooms in (default 1) */
        zoom?: VideoValue;
        /** Turns; positive turns the picture clockwise (default 0) */
        rotate?: VideoValue;
        /** Horizontal move as a fraction of the width; positive moves right (default 0) */
        shiftX?: VideoValue;
        /** Vertical move as a fraction of the height; positive moves up (default 0) */
        shiftY?: VideoValue;
        /** What lies beyond the frame border (default \`'clamp'\`) */
        edge?: 'clamp' | 'repeat' | 'mirror';
    }): VideoSignal;
    /** Stores a signal for the next frame to read, and returns it. */
    write(signal: VideoSignal): VideoSignal;
}

${generateVideoNamespace()}
`;

/** `TrackTap`'s members, documented as what `$trackTap` returns. */
function renderTrackTapMembers(): string {
    const trackTap = GLOBAL_ENTRIES.find((e) => e.name === '$trackTap');
    return (trackTap?.returns?.members ?? [])
        .flatMap(({ name, description, meta }) => [
            ...jsDocComment([withMeta(description, meta)], '  '),
            `  readonly ${name.replace(/^\./, '')}: CollectionWithRange;`,
        ])
        .join('\n');
}

/**
 * The JSDoc of `fn` above one `declaration`: its docs
 * (`renderDocContentJsDoc`) after any `lead` lines, then the `@param`s it
 * declares from the `from`th on.
 */
function functionJsDoc(
    fn: FunctionDoc,
    declaration: string,
    { indent = '', lead = [] as string[], from = 0 } = {},
): string[] {
    const { name, params } = parseDeclaration(declaration);
    const returns =
        fn.returns && withMeta(fn.returns.description, fn.returns.meta);
    return jsDocComment(
        [
            ...lead,
            ...renderDocContentJsDoc(fn.docs, name),
            ...params
                .slice(from)
                .filter((p) => p.name in fn.params)
                .map((p) => {
                    const { description, meta } = readDocText(
                        fn.params[p.name],
                    );
                    return `@param ${p.name} ${withMeta(description, meta)}`;
                }),
            ...(returns ? [`@returns ${returns}`] : []),
        ],
        indent,
    );
}

/** Every overload of each of `fns`, documented, as a global function. */
function renderFunctions(fns: FunctionDoc[]): string[] {
    return fns.flatMap((fn) =>
        fn.declarations.flatMap((declaration) => [
            ...functionJsDoc(fn, declaration),
            `function ${declaration};`,
            '',
        ]),
    );
}

export function buildLibSource(schemas: Schemas): string {
    const schemaLib = generateDSL(schemas);
    return `/* oxlint-disable */\ndeclare global {\n${BASE_LIB_SOURCE}\n\n${schemaLib}\n}\n\nexport {};\n`;
}
interface NamespaceNode {
    namespaces: Map<string, NamespaceNode>;
    classes: Map<string, Schema>;
    order: Array<{ kind: 'namespace' | 'class'; name: string }>;
}

function makeNamespaceNode(): NamespaceNode {
    return {
        classes: new Map(),
        namespaces: new Map(),
        order: [],
    };
}

function buildTreeFromSchemas(schemas: Schemas): NamespaceNode {
    const root = makeNamespaceNode();

    for (const moduleSchema of schemas) {
        const fullName = String(moduleSchema.name).trim();
        if (!fullName) {
            throw new Error('ModuleSchema is missing a non-empty name');
        }

        const { paramsSchema } = moduleSchema;
        if (!paramsSchema || typeof paramsSchema !== 'object') {
            throw new Error(`ModuleSchema ${fullName} is missing paramsSchema`);
        }

        const parts = fullName.split('.').filter((p: string) => p.length > 0);
        if (parts.length === 0) {
            throw new Error(`Invalid ModuleSchema name: ${fullName}`);
        }

        const className = parts[parts.length - 1];
        const namespacePath = parts.slice(0, -1);

        let node = root;
        for (const ns of namespacePath) {
            let child = node.namespaces.get(ns);
            if (!child) {
                child = makeNamespaceNode();
                node.namespaces.set(ns, child);
                node.order.push({ kind: 'namespace', name: ns });
            }
            node = child;
        }

        if (node.classes.has(className)) {
            throw new Error(`Duplicate class name detected: ${fullName}`);
        }

        node.classes.set(className, moduleSchema);
        node.order.push({ kind: 'class', name: className });
    }

    return root;
}

function capitalizeName(name: string): string {
    if (!name) {
        return name;
    }
    return name.charAt(0).toUpperCase() + name.slice(1);
}

/**
 * The type of a single output: ranged when its range is declared or set by
 * the module's own params. One whose range follows its inputs is ranged only
 * through the ranged overloads of {@link renderRangedChainMethod}.
 */
function getOutputType(output: { name: string; bounds?: string }): string {
    return output.bounds === 'fixed' || output.bounds === 'params'
        ? 'CollectionWithRange'
        : 'Collection';
}

/**
 * The positional params `moduleSchema`'s output takes its range from, each
 * mapped to whether it must hold a constant, when its output can be ranged
 * by them: a single `bounds = input` output, and the first positional among
 * the tagged params, every one positional. `null` otherwise.
 */
function rangedParams(moduleSchema: Schema): Map<string, boolean> | null {
    const outputs = moduleSchema.outputs ?? [];
    if (outputs.length !== 1 || outputs[0].bounds !== 'input') {
        return null;
    }
    const tagged = new Map(
        moduleSchema.signalParams
            .filter((param) => param.carriesBounds)
            .map((param) => [param.name, param.boundsNeedConstant]),
    );
    const positionals = (moduleSchema.positionalArgs ?? []).map((a) => a.name);
    const receiver = positionals[0];
    if (!receiver || tagged.get(receiver) !== false) {
        return null;
    }
    return [...tagged.keys()].every((name) => positionals.includes(name))
        ? tagged
        : null;
}

/**
 * `args` typed for a ranged result: `receiver` ranged, the other params
 * `tagged` names ranged or constant, or constant where they must be, and a
 * bounding param, if set, to the values that keep the range.
 */
function rangedArgs(
    moduleSchema: Schema,
    args: string[],
    tagged: Map<string, boolean>,
    receiver: string | null,
): string[] {
    const typed = args.map((arg) => {
        const name = arg.match(/^(\w+)\??:/)?.[1];
        if (!name || !tagged.has(name)) {
            return arg;
        }
        const type =
            name === receiver
                ? 'OrArray<CollectionWithRange>'
                : tagged.get(name)
                  ? 'OrArray<ConstantSignal>'
                  : 'OrArray<RangedSignal>';
        return `${name}${arg.includes('?:') ? '?' : ''}: ${type}`;
    });
    const bounding = moduleSchema.boundingParam;
    if (!bounding) {
        return typed;
    }
    return withConfig(typed, (config) =>
        config.replace(
            new RegExp(`\\b${bounding.name}\\?: [^;}]+`),
            `${bounding.name}?: ${valuesType(bounding.values)}`,
        ),
    );
}

/**
 * The chain method of `moduleSchema` on a ranged collection: an overload
 * keeping the range when its arguments allow, then the general one where
 * other arguments reach it. `null` for a module without one.
 */
function renderRangedChainMethod(
    moduleSchema: Schema,
    indent: string,
): string[] | null {
    const tagged = rangedParams(moduleSchema);
    if (!tagged) {
        return null;
    }
    const { args, doc } = chainSignature(moduleSchema, indent);
    const ranged = rangedArgs(moduleSchema, args, tagged, null);
    const lines = [
        ...doc,
        `${indent}${chainMethodName(moduleSchema.name)}(${ranged.join(', ')}): Ranged;`,
    ];
    // The ranged overload already covers a declared range while the receiver
    // is the declaring module's only tagged param, as for `$clamp`.
    if (ranged.some((arg, i) => arg !== args[i])) {
        lines.push(
            ...renderChainMethod(
                moduleSchema,
                indent,
                false,
                'Plain',
                'Ranged',
            ),
        );
    }
    return lines;
}

/** The lib types of a pattern, each a root of `$cycle`'s outputs. */
export const PATTERN_TYPES = ['Pattern', 'ScalePattern'] as const;

/**
 * The multi-output interface of `moduleSchema` as the type checker names it:
 * qualified by the namespaces the module is declared in. `_clock` is declared
 * at the top level.
 */
export function qualifiedMultiOutputInterfaceName(moduleSchema: {
    name: string;
}): string {
    const namespaces = moduleSchema.name
        .split('.')
        .filter((p: string) => p.length > 0)
        .slice(0, -1);
    return [...namespaces, getMultiOutputInterfaceName(moduleSchema)].join('.');
}

/**
 * `type` as a chain from a receiver gives it: the receiver's own plain or
 * ranged type for a collection, anything else unchanged.
 */
function receiverType(type: string, plain: string, ranged: string): string {
    return { Collection: plain, CollectionWithRange: ranged }[type] ?? type;
}

/** Appended to a multi-output interface's name for its chained form. */
const CHAINED_SUFFIX = 'Of';

/**
 * The qualified name of the type a multi-output module's chain method
 * returns: the signal it was chained from with the module's outputs, each
 * typed as that signal too, so a chain from a bus or group stays one
 * through any output.
 */
export function chainedOutputsTypeName(moduleSchema: { name: string }): string {
    return `${qualifiedMultiOutputInterfaceName(moduleSchema)}${CHAINED_SUFFIX}`;
}

/**
 * Generate interface name for multi-output modules
 */
function getMultiOutputInterfaceName(moduleSchema: { name: string }): string {
    const parts = moduleSchema.name
        .split('.')
        .filter((p: string) => p.length > 0);
    const baseName = parts[parts.length - 1];
    const baseNameWithoutPrefix = baseName.startsWith('$')
        ? baseName.slice(1)
        : baseName;
    return `${capitalizeName(baseNameWithoutPrefix)}Outputs`;
}

/**
 * Generate interface definition for multi-output modules.
 * The interface extends the type of the output a `cv` input takes, and has
 * every output by name behind an underscore.
 */
function generateMultiOutputInterface(
    moduleSchema: Schema,
    indent: string,
): string[] {
    const outputs = moduleSchema.outputs || [];
    if (outputs.length <= 1) {
        return [];
    }

    const defaultOutput = outputs.find((o) => o.default) || outputs[0];
    // Read as a signal itself, the module is the output a `cv` input takes.
    const cvOutput = chooseOutput(outputs, 'cv');
    const baseType = getOutputType(outputs.find((o) => o.name === cvOutput)!);

    const interfaceName = getMultiOutputInterfaceName(moduleSchema);
    // Chained from a bus or group, the module and each output keep the
    // receiver's own type.
    const chained = (type: string) => receiverType(type, 'Plain', 'Ranged');
    const chainedName = `${interfaceName}${CHAINED_SUFFIX}`;

    const lines = jsDocComment(
        [
            `The outputs of ${moduleSchema.name} chained from a signal of type \`Plain\`, or \`Ranged\` where one has a range.`,
        ],
        indent,
    );
    lines.push(
        `${indent}export type ${chainedName}<Plain, Ranged> = ${chained(baseType)} & {`,
        `${indent}  /** This module's outputs, each followed by the lanes of \`other\`. */`,
        `${indent}  concat(other: ${baseType}): ${chainedName}<Plain, Ranged>;`,
    );
    for (const output of outputs) {
        const doc = withMeta(output.description, formatOutputMeta(output));
        if (doc) {
            lines.push(`${indent}  /** ${doc} */`);
        }
        lines.push(
            `${indent}  readonly _${outputName(output.name)}: ${chained(getOutputType(output))};`,
        );
    }
    lines.push(`${indent}};`);
    lines.push(
        ...jsDocComment(
            [
                `Output type for ${moduleSchema.name} module.`,
                'Used directly, it stands for the output that fits the input it',
                `is plugged into (\`_${outputName(defaultOutput.name)}\` unless another fits better); an output named`,
                'itself is always that output.',
            ],
            indent,
        ),
        `${indent}export interface ${interfaceName} extends ${chainedName}<Collection, CollectionWithRange> {}`,
    );
    return lines;
}

/**
 * Get the return type for a module factory based on its outputs
 */
function getFactoryReturnType(moduleSchema: Schema): string {
    const outputs = moduleSchema.outputs || [];

    if (outputs.length === 0) {
        return 'void';
    } else if (outputs.length === 1) {
        return getOutputType(outputs[0]);
    }

    return getMultiOutputInterfaceName(moduleSchema);
}

/**
 * Param docs render as single JSDoc list items, so a doc comment wrapped over
 * several lines is joined into one.
 */
function paramDescription(description: string): string {
    return description
        .split(/\s*\r?\n\s*/)
        .filter(Boolean)
        .join(' ');
}

/** Options listed in full up to this many; beyond it, by name only. */
const MAX_DESCRIBED_OPTIONS = 8;

/**
 * A param's enum options on one line: `Options: \`a\` what a does; \`b\` …`.
 * TypeScript strips the indentation of JSDoc tag lines, so options cannot
 * nest as a list under their param.
 */
function optionsText(
    propSchema: JSONSchema,
    paramsSchema: Schema['paramsSchema'],
): string | undefined {
    const variants = getEnumVariants(propSchema, paramsSchema);
    if (!variants || !variants.some((v) => v.description)) {
        return undefined;
    }
    if (variants.length > MAX_DESCRIBED_OPTIONS) {
        return `Options: ${variants.map((v) => `\`${v.value}\``).join(', ')}.`;
    }
    const items = variants.map((v) =>
        v.description ? `\`${v.value}\` ${v.description}` : `\`${v.value}\``,
    );
    return `Options: ${items.join('; ')}.`;
}

/**
 * Build the trailing `config?: { ... }` argument shared by the factory-function
 * and chain-method renderers: every non-positional param, plus an optional
 * `id`. `config` is required only when some non-positional param is required.
 * Returns the rendered argument and the nested `@param config` doc lines.
 */
function buildConfigArg(moduleSchema: Schema): {
    arg: string;
    paramDocs: string[];
} {
    const { paramsSchema } = moduleSchema;
    const schemaProperties = paramsSchema.properties as
        | Record<string, JSONSchema | undefined>
        | undefined;
    const schemaRequired: readonly string[] = paramsSchema.required || [];
    const positionalKeys = new Set(
        (moduleSchema.positionalArgs || []).map((a) => a.name),
    );
    const allParamKeys = Object.keys(paramsSchema.properties || {});

    const configProps: string[] = [];
    const paramDocs: string[] = [];

    for (const key of allParamKeys) {
        if (positionalKeys.has(key)) {
            continue;
        }
        const propSchema = schemaProperties?.[key];
        if (!propSchema) {
            continue;
        }
        const type = schemaToTypeExpr(propSchema, paramsSchema);
        const optionalMark = schemaRequired.includes(key) ? '' : '?';
        configProps.push(`${key}${optionalMark}: ${type}`);

        const description = propSchema?.description;
        const doc = withMeta(
            description && paramDescription(description),
            paramMeta(moduleSchema, key),
        );
        const options = optionsText(propSchema, paramsSchema);
        if (doc || options) {
            paramDocs.push(
                `  - ${key} - ${[doc, options].filter(Boolean).join(' · ')}`,
            );
        }
    }

    configProps.push(`id?: string`);

    const configType = `{ ${configProps.join('; ')} }`;

    // Config is required if any non-positional param is required
    const hasRequiredConfigProps = allParamKeys.some(
        (key: string) =>
            !positionalKeys.has(key) && schemaRequired.includes(key),
    );
    const configOptional = hasRequiredConfigProps ? '' : '?';
    return { arg: `config${configOptional}: ${configType}`, paramDocs };
}

/**
 * Build the parameter list and JSDoc lines shared by the factory-function and
 * chain-method renderers. `positionalStart` drops leading positionals (1 for
 * a chain method, whose receiver is the first argument); `leadDocLines` are
 * prepended before the module's own documentation. The module's docs
 * (`renderDocContentJsDoc`) and every `@param` are always included, so the
 * factory and chain surfaces carry identical docs and cannot drift. An
 * experimental module leads with `EXPERIMENTAL_NOTICE`, which signature help
 * shows (it omits tags), and carries an `@experimental` tag, which marks its
 * completion entries.
 */
function buildSignature(
    moduleSchema: Schema,
    opts: {
        positionalStart: number;
        leadDocLines?: string[];
    },
): { args: string[]; docLines: string[] } {
    const { paramsSchema } = moduleSchema;
    const schemaProperties = paramsSchema.properties as
        | Record<string, JSONSchema | undefined>
        | undefined;
    const schemaRequired: readonly string[] = paramsSchema.required || [];
    const positionals = (moduleSchema.positionalArgs || []).slice(
        opts.positionalStart,
    );
    const requiredness = positionals.map((a) =>
        schemaRequired.includes(a.name),
    );

    const args: string[] = [];
    const docLines: string[] = moduleSchema.experimental
        ? [EXPERIMENTAL_NOTICE, '']
        : [];
    docLines.push(...(opts.leadDocLines ?? []));
    docLines.push(
        ...renderDocContentJsDoc(moduleSchema.docs, moduleSchema.name),
    );

    for (let i = 0; i < positionals.length; i++) {
        const arg = positionals[i];
        const propSchema = schemaProperties?.[arg.name];
        const type = propSchema
            ? schemaToTypeExpr(propSchema, paramsSchema)
            : 'any';

        if (requiredness[i]) {
            args.push(`${arg.name}: ${type}`);
        } else {
            // Stay before any required arg: emit `| undefined` rather than `?`.
            const allSubsequentOptional = requiredness
                .slice(i + 1)
                .every((r) => !r);
            args.push(
                allSubsequentOptional
                    ? `${arg.name}?: ${type}`
                    : `${arg.name}: ${type} | undefined`,
            );
        }

        const description = propSchema?.description;
        const doc = withMeta(
            description && paramDescription(description),
            paramMeta(moduleSchema, arg.name),
        );
        docLines.push(doc ? `@param ${arg.name} ${doc}` : `@param ${arg.name}`);

        if (propSchema) {
            const options = optionsText(propSchema, paramsSchema);
            if (options) {
                docLines.push(options);
            }
        }
    }

    const { arg: configArg, paramDocs } = buildConfigArg(moduleSchema);
    args.push(configArg);
    if (paramDocs.length > 0) {
        docLines.push(`@param config Configuration object`);
        docLines.push(...paramDocs);
    } else {
        docLines.push(`@param config Configuration object`);
    }
    if (moduleSchema.experimental) {
        docLines.push('@experimental');
    }

    const outputs = moduleSchema.outputs ?? [];
    // A module without a default output returns all its outputs.
    const main = outputs.find((o) => o.default);
    const doc = main && withMeta(main.description, formatOutputMeta(main));
    if (doc) {
        docLines.push(`@returns ${doc}`);
    }

    return { args, docLines };
}

/** The literal type of `values`, quoted as the schema's enums render. */
function valuesType(values: string[]): string {
    return values.map((value) => JSON.stringify(value)).join(' | ');
}

/** `args` with the trailing config arg passed through `edit`. */
function withConfig(
    args: string[],
    edit: (config: string) => string,
): string[] {
    return [...args.slice(0, -1), edit(args[args.length - 1])];
}

/**
 * The args of `moduleSchema`'s overload for the range it declares with both
 * declaring params given, whatever its input. `null` for a module that
 * declares none.
 */
function declaredRangeArgs(
    moduleSchema: Schema,
    args: string[],
): string[] | null {
    const declared = moduleSchema.declaredRange;
    if (!declared) {
        return null;
    }
    return withConfig(args, (config) => {
        let required = config.replace(/^config\?:/, 'config:');
        for (const name of declared) {
            required = required.replace(
                new RegExp(`\\b${name}\\?:`),
                `${name}:`,
            );
        }
        return required;
    });
}

/** The lines a chain method's docs open with, before its module's own. */
function chainLead(name: string): string[] {
    return [
        `Chain through \`${name}\` with this signal as its first`,
        `argument. Equivalent to \`${name}(this, ...)\`.`,
        '',
    ];
}

/**
 * The args and doc comment of `moduleSchema`'s chain method: the module's
 * factory without its first positional, which the receiver supplies, and the
 * module's docs, like the factory.
 */
function chainSignature(
    moduleSchema: Schema,
    indent: string,
): { args: string[]; doc: string[] } {
    const { args, docLines } = buildSignature(moduleSchema, {
        positionalStart: 1,
        leadDocLines: chainLead(moduleSchema.name),
    });
    return {
        args,
        doc: jsDocComment(docLines, indent),
    };
}

/**
 * Render the chain method of `moduleSchema`, with the overload for a range
 * it declares first unless `withDeclared` is false.
 *
 * Only called on schemas passing `qualifiesForChain`, which guarantees
 * `chainMethodName` yields a valid TS identifier for the interface member.
 */
function renderChainMethod(
    moduleSchema: Schema,
    indent: string,
    withDeclared = true,
    // What a plain and a ranged signal result are typed as; a chain from
    // `$bus()` keeps its own type, through the chain interfaces' `Plain` and
    // `Ranged` parameters.
    plain = 'Collection',
    ranged = 'CollectionWithRange',
): string[] {
    const methodName = chainMethodName(moduleSchema.name);
    const { args, doc } = chainSignature(moduleSchema, indent);
    // The chain interfaces sit outside the module's namespace, so a
    // multi-output interface is named in full.
    const returnType =
        (moduleSchema.outputs ?? []).length > 1
            ? `${chainedOutputsTypeName(moduleSchema)}<${plain}, ${ranged}>`
            : receiverType(getFactoryReturnType(moduleSchema), plain, ranged);
    const lines: string[] = [];
    const declared = declaredRangeArgs(moduleSchema, args);
    if (declared && withDeclared) {
        lines.push(
            ...doc,
            `${indent}${methodName}(${declared.join(', ')}): ${ranged};`,
        );
    }
    lines.push(
        ...doc,
        `${indent}${methodName}(${args.join(', ')}): ${returnType};`,
    );
    return lines;
}

function renderFactoryFunction(
    moduleSchema: Schema,
    _interfaceName: string,
    indent: string,
): string[] {
    const functionName = moduleSchema.name.split('.').pop()!;
    const { args, docLines } = buildSignature(moduleSchema, {
        positionalStart: 0,
    });
    const returnType = getFactoryReturnType(moduleSchema);

    const doc = jsDocComment(docLines, indent);
    const overload = (overloadArgs: string[], type: string) => [
        ...doc,
        `${indent}export function ${functionName}(${overloadArgs.join(', ')}): ${type};`,
    ];
    const lines: string[] = [];
    const tagged = rangedParams(moduleSchema);
    if (tagged) {
        const receiver = moduleSchema.positionalArgs[0].name;
        lines.push(
            ...overload(
                rangedArgs(moduleSchema, args, tagged, receiver),
                'CollectionWithRange',
            ),
        );
    }
    const declared = declaredRangeArgs(moduleSchema, args);
    if (declared) {
        lines.push(...overload(declared, 'CollectionWithRange'));
    }
    lines.push(...overload(args, returnType));

    return lines;
}

function renderInterface(classSpec: Schema, indent: string): string[] {
    const lines: string[] = [];

    // Generate multi-output interface if needed
    const multiOutputInterface = generateMultiOutputInterface(
        classSpec,
        indent,
    );
    if (multiOutputInterface.length > 0) {
        lines.push(...multiOutputInterface);
        lines.push('');
    }

    // Render the factory function
    lines.push(...renderFactoryFunction(classSpec, '', indent));
    return lines;
}

function renderTree(node: NamespaceNode, indentLevel: number = 0): string[] {
    const indent = '  '.repeat(indentLevel);
    const lines: string[] = [];

    for (const item of node.order) {
        if (item.kind === 'class') {
            const classSpec = node.classes.get(item.name);
            if (!classSpec) {
                continue;
            }
            lines.push(...renderInterface(classSpec, indent));
            lines.push('');
            continue;
        }

        const child = node.namespaces.get(item.name);
        if (!child) {
            continue;
        }
        lines.push(`${indent}export namespace ${item.name} {`);
        lines.push(...renderTree(child, indentLevel + 1));
        lines.push(`${indent}}`);
        lines.push('');
    }

    // Trim extra blank lines at this level.
    while (lines.length > 0 && lines[lines.length - 1] === '') {
        lines.pop();
    }
    return lines;
}

/**
 * `$enosc.scale.*`, merged into the `$enosc` module function: one constant
 * per preset in the engine's `EnoscScale` schema, plus `custom`. The executor
 * builds the values.
 */
function renderEnoscNamespace(schemas: Schemas): string[] {
    const presets = enoscScalePresets(schemas).flatMap(
        ({ name, description }) => [
            `    /** ${description} */`,
            `    readonly ${name}: EnoscScale;`,
        ],
    );
    return [
        'export namespace $enosc {',
        '  /**',
        "   * The 4ms Ensemble Oscillator's scales for `$enosc`'s `scale` param:",
        '   * its 12-TET and Octave banks as constants, and `custom`. A scale repeats',
        '   * at its last note, which is the octave unless noted.',
        '   *',
        '   * @example',
        "   * keys: $enosc('c3', $enosc.scale.harm4to7)",
        '   */',
        '  export const scale: {',
        ...presets,
        '    /**',
        "     * Your own scale, like the hardware's learn mode.",
        '     *',
        '     * @param notes Space-separated notes in semitones, in any order and',
        '     * fractional if you like (`"0 3 7 10"`); the lowest is the scale\'s first',
        '     * note. Up to 32 notes (31 when `repeatsOctave` is true), spanning at',
        '     * most 150 semitones.',
        '     * @param repeatsOctave `true` repeats the notes every octave (every',
        '     * two octaves if they span more than one, and so on). `false` makes the',
        '     * highest note the period the scale repeats at, so `"0 3 7"` repeats',
        '     * every fifth.',
        '     *',
        '     * @example',
        "     * keys: $enosc('c3', $enosc.scale.custom('0 2 3 7 9', true))",
        '     */',
        '    custom(notes: string, repeatsOctave: boolean): EnoscScale;',
        '  };',
        '}',
    ];
}

export function generateDSL(schemas: Schemas): string {
    // Modules named with a leading `_` are internal, and $buffer has a custom
    // declaration below.
    const userFacingSchemas = schemas.filter(
        (s) => !s.name.startsWith('_') && s.name !== '$buffer',
    );
    const tree = buildTreeFromSchemas(userFacingSchemas);
    const lines = renderTree(tree, 0);

    // $clock is a pre-configured clock instance available to users.
    // Because the _clock factory is filtered from userFacingSchemas, its multi-output
    // Interface won't have been generated by renderTree. Generate it here
    // So that `$clock` has a proper type.
    const clockSchema = schemas.find((s) => s.name === '_clock');
    if (clockSchema) {
        const clockInterface = generateMultiOutputInterface(clockSchema, '');
        if (clockInterface.length > 0) {
            lines.push('');
            lines.push(...clockInterface);
        }
        lines.push('');
        lines.push('/** Global clock module running at 120 BPM by default. */');
        const clockReturnType = getFactoryReturnType(clockSchema);
        lines.push(`export const $clock: ${clockReturnType};`);
    }

    if (schemas.some((s) => s.name === '$enosc')) {
        lines.push('');
        lines.push(...renderEnoscNamespace(schemas));
    }

    const signalSchema = schemas.find((s) => s.name === '$signal');
    if (signalSchema) {
        lines.push('');
        const signalReturnType = getFactoryReturnType(signalSchema);
        lines.push(
            '/** Live audio input in stereo lanes: input channels 1 and 2 are the first lane, 3 and 4 the second, and an odd last channel sits in the centre. */',
        );
        lines.push(`export const $input: Readonly<${signalReturnType}>;`);
        lines.push(
            '/** Live audio input, one mono lane per input channel on the current device. */',
        );
        lines.push(`export const $inputMono: Readonly<${signalReturnType}>;`);
    }

    lines.push('');
    lines.push(
        '/** Create a buffer module that captures an input signal into a circular audio buffer. */',
    );
    lines.push(
        'export function $buffer(input: Signal, lengthSeconds: number, config?: { id?: string }): BufferOutputRef;',
    );
    lines.push('');
    const builtModules = dslModules(schemas as unknown as ModuleSchema[]);
    lines.push(...renderFunctions(builtModules));

    // Chain methods. The qualifying set matches the runtime's exactly: both
    // apply `qualifiesForChain` to the same schemas.
    const chainSchemas = userFacingSchemas.filter((s) =>
        qualifiesForChain(processModuleSchema(s as unknown as ModuleSchema)),
    );

    // Flat modules (`$lpf`) become direct members; dotted modules
    // (`$unstable.shape.fold`) nest under a namespace tree of any depth
    // (`.unstable.shape.fold`), mirroring the global namespace and keeping leaf
    // names like `fold` from colliding with flat modules like `$fold`.
    interface ChainTypeNode {
        leaves: Schema[];
        children: Map<string, ChainTypeNode>;
    }
    const chainRoot: ChainTypeNode = { leaves: [], children: new Map() };
    for (const s of chainSchemas) {
        const segments = s.name.slice(1).split('.');
        let node = chainRoot;
        for (const seg of segments.slice(0, -1)) {
            let child = node.children.get(seg);
            if (child === undefined) {
                child = { leaves: [], children: new Map() };
                node.children.set(seg, child);
            }
            node = child;
        }
        node.leaves.push(s);
    }
    const pascal = (s: string): string =>
        s
            .split(/[^a-zA-Z0-9]/)
            .filter(Boolean)
            .map((w) => w[0].toUpperCase() + w.slice(1))
            .join('');
    const sortedChildren = (node: ChainTypeNode): string[] =>
        [...node.children.keys()].sort();

    // Emit a sub-interface for every namespace node, so `.unstable.shape.fold`
    // yields `ModuleChainUnstable { shape: ModuleChainUnstableShape }` and
    // `ModuleChainUnstableShape { fold(...) }`.
    const emitNamespaceInterfaces = (
        node: ChainTypeNode,
        path: string[],
    ): void => {
        for (const seg of sortedChildren(node)) {
            emitNamespaceInterfaces(node.children.get(seg)!, [...path, seg]);
        }
        const dotted = path.join('.');
        lines.push('');
        lines.push(`/** The \`$${dotted}\` modules as chain methods. */`);
        lines.push(
            `interface ModuleChain${pascal(dotted)}<Plain = Collection, Ranged = CollectionWithRange> {`,
        );
        for (const s of node.leaves) {
            lines.push(...renderChainMethod(s, '  ', true, 'Plain', 'Ranged'));
        }
        for (const seg of sortedChildren(node)) {
            lines.push(
                `  readonly ${seg}: ModuleChain${pascal([...path, seg].join('.'))}<Plain, Ranged>;`,
            );
        }
        lines.push('}');
    };
    for (const seg of sortedChildren(chainRoot)) {
        emitNamespaceInterfaces(chainRoot.children.get(seg)!, [seg]);
    }

    lines.push('');
    lines.push(
        ...jsDocComment([
            'The chain methods of a collection: every module whose first',
            'argument is a {@link Poly<Signal>}, with the receiver as that argument.',
            "@example $sine(0).lpf('100hz')  // ≡ $lpf($sine(0), '100hz')",
        ]),
    );
    lines.push(
        'interface ModuleChain<Plain = Collection, Ranged = CollectionWithRange> {',
    );
    for (const s of chainRoot.leaves) {
        lines.push(...renderChainMethod(s, '  ', true, 'Plain', 'Ranged'));
    }
    for (const seg of sortedChildren(chainRoot)) {
        lines.push(
            `  readonly ${seg}: ModuleChain${pascal(seg)}<Plain, Ranged>;`,
        );
    }
    for (const fn of builtModules.filter((m) => m.chainable)) {
        for (const declaration of fn.declarations) {
            lines.push(
                ...functionJsDoc(fn, declaration, {
                    from: 1,
                    indent: '  ',
                    lead: chainLead(parseDeclaration(declaration).name),
                }),
                `  ${chainDeclaration(declaration, 'Plain')};`,
            );
        }
    }
    lines.push('}');

    lines.push('');
    // Merged into the class, so each overload narrows BaseCollection's method.
    lines.push('/** Chain methods that keep a ranged signal ranged. */');
    lines.push('interface RangedCollection<Plain, Ranged extends Plain> {');
    for (const s of chainRoot.leaves) {
        const overloads = renderRangedChainMethod(s, '  ');
        if (overloads) {
            lines.push(...overloads);
        }
    }
    lines.push('}');

    return lines.join('\n') + '\n';
}
