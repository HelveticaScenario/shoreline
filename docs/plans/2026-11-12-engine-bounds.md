# Engine bounds

A signal's bounds are the two values it swings between. The engine carries them on every bounded output, per channel and per sample, written by the module that writes the samples and read through the same paths, so a reader at sample `i` sees the bounds of sample `i` in Block mode, in Sample mode and through a feedback cycle's one-sample delay. Consumers read their input's bounds at run time: `$remap` maps from them when its input range is unset, and `$erratic` keeps to them. The DSL keeps only whether a signal is bounded, for types; nothing builds modules to track bounds.

## `Bounds`

```rust
#[derive(Clone, Copy, PartialEq)]
pub struct Bounds { pub start: f32, pub end: f32 }
```

An ordered pair: the values a signal takes at the start and end of its swing. `.range(5, 0)` is `(5, 0)`, and a `$remap` reading it as its input range maps 5 to its `outMin`, so `.range(5, 0).range(0, 1)` flips the signal back.

- Ends may be infinite. `Bounds::UNBOUNDED` is `(-inf, inf)`, `point(v)` is `(v, v)`, and `is_bounded()` holds when both ends are finite. `lo()`/`hi()` sort the ends.
- `map(f)` applies a non-decreasing `f` to both ends and keeps the orientation.
- A binary rule maps the endpoints through the operation when either side is a point, so orientation follows the operation (`5 - x` flips, `x * -2` flips), and gives the sorted hull of the corners otherwise. A point operand is its value at that slot, glided value included, so orientation never jumps mid-glide.

## Which outputs are bounded

Each output declares in `#[output]` how its bounds arise:

| Kind      | Declared as                                              | Bounds                                                  |
| --------- | -------------------------------------------------------- | ------------------------------------------------------- |
| unbounded | (default)                                                | `UNBOUNDED`                                             |
| fixed     | `range = (a, b)`; every `gate`/`trig` output is `(0, 5)` | constant                                                |
| params    | `bounds = params`                                        | from the module's own params: `$remap`, `$wrap`         |
| input     | `bounds = input`                                         | from the inputs tagged `#[input(…, bounds)]` and params |

Whether an output is bounded is settled when the patch is built (`Patch::settle_bounds`, on the main thread once every module is constructed): fixed and params outputs are bounded; an input output is bounded once every tagged input is, where a constant is bounded and a cable is as bounded as its producer's port. Growing the set until nothing joins takes the least fixpoint, so a cycle that only feeds itself never becomes bounded and a rule can never widen around a loop; a cycle through `$remap` is bounded from there on. `#[params(bounded_when = …)]` replaces the tagged-inputs test where a module needs another (`$div`, `$clamp`, `$erratic`).

A bounded output stores its bounds only when something reads them: a module that reads an input's bounds for itself (`#[params(reads_bounds = …)]`: `$remap`'s unset input range, `$erratic`'s unset range and mode), or a read output whose bounds come from it, through its tagged inputs. Only those outputs pay any cost; the rest read `UNBOUNDED`, and a patch with no reader runs exactly as without bounds. A rule runs only for an output that stores, so it never stores a non-finite end; a module rebuilt on its own between patch updates (a slider move) takes the storage its predecessor had.

The DSL computes the same flag as it builds and errs only towards unranged (a deferred output is unranged until set), since a signal the DSL calls ranged but the engine leaves unbounded would make a two-argument `.range` map from ±5 silently. A parity test runs the example patches and cycle fixtures and checks that every output the DSL calls ranged is bounded by the engine's pass, exposed through N-API.

"Constant" below means every channel of the input is a fixed voltage, in the engine and in the DSL alike (a plain number, not a `$c(2)` cable).

Two modules have a statically narrower rule:

- `$div` is bounded only when its divisor is a constant. Its glide works on the reciprocal (0 for a divisor of 0), and the bounds are the input's mapped through that reciprocal.
- `$clamp` is `input` kind; its rule is `map(clamp)`, which bounds an unbounded input to `(min, max)` when both are set. It is bounded statically when its input is or both `min` and `max` are set.

## Storage, writes and reads

- **Storage.** A read output whose kind is params or input has a bounds store in its `BlockPort` beside the samples, allocated on the main thread before the patch is connected and initialised to 0 V's bounds, as the samples hold 0 V: `block_size × channels` `Bounds`. A fixed or unbounded output has none and reads its constant. The store lives in `XBlockOutputs`, so a patch swap carries it with the samples, and `same_layout` compares it.
- **Runs.** Bounds rarely move within a block, so a port's bounds store keeps the slots from 0 that hold one row as that row and a count, and writes the slots out only once a different row arrives. A single-slot read checks the run first; at the start of a block a run that covered the whole block writes out its last slot, which a feedback cycle reads back with its one-sample delay.
- **Writes.** `update` writes a channel's bounds beside its value (`PolyOutput::set_bounds`) when the output stores them, and Sample-mode `copy_from_inner` copies them into the store with the samples. `process_block` writes them before the samples. A debug build asserts after every render that each store has its bounds written as far as the samples. f64 outputs keep f32 bounds; f64 modules run `update`, which writes them.
- **Reads.** `Sampleable::get_bounds_at(port, ch, index)` (default `UNBOUNDED`, for test stubs and `AudioIn`) resolves through `readable_slot` exactly as `get_value_at` does, so a delayed read in a cycle gets the delayed slot's bounds. `Signal::get_bounds()` gives `point(v)` for a constant and reads the producer for a cable, in place through the `CableView` when possible. `PolySignal::get_bounds(ch)` reads one slot. `PolySignal::block_bounds(range, ch)` gives a channel's bounds when they hold still over the range: a constant, a fixed or unbounded producer, or a producer whose run covers the range; otherwise `None`, and the block path declines as it does during a glide, so the wrapper falls back to `update` for the samples whose bounds move. A block path writes its bounds before its samples (`BlockPort::fill_bounds_with`, or `block_input::map_bounds` when they follow a moving param), so a decline leaves nothing written.

## Rules

Each rule is declared once and both paths call it.

- **Operators.** `Combine` gains `fn bounds(input: Bounds, operand: Bounds) -> Bounds`, called by `operator::update` and `process_block` with the operand as glided: `$add`, `$sub`, `$mul` by endpoint mapping or corner hull; `$amp` as `$mul` by `factor / 5`; `$gain` as `$mul` by the audio taper of `level`; `$div` by the reciprocal; `$exp` maps the input's ends at a point exponent and takes the corner hull at a swinging one.
- **Unary, non-decreasing.** `$round`, `$floor`, `$ceil`, `$curve`, `$clamp` use `Bounds::map` with the same function as their samples. `$slew` passes its input's bounds through. `$sah` and `$tah` latch the input's bounds with the sample they hold, so a held sample keeps the bounds it was taken under.
- **Params.** `$remap`: `(outMin, outMax)`. This is a declaration, not a hull: a four-argument remap of an input that leaves its stated range leaves this one too, as it always has. `$wrap`: `(min, max)` sorted.
- **Pass-through.** `$signal` is `input` kind over its value: `point(v)` for a constant, its cable's bounds otherwise.
- **Two inputs.** `_crossfade`: the sorted hull of dry and wet.
- **`$erratic`.** In `blend` and `clamp` its output bounds are `(min, max)` sorted; in `shift` it is unbounded.

## Consumers and the DSL

- `$remap`'s `inMin`/`inMax` are optional; unset, they read the input's bounds in order, and an input without a range (unbounded, or holding one value) reads as `(-5, 5)`. Setting an unset limit glides from the range in use; unsetting one takes the input's range at once.
- `_erratic`'s `min`/`max` unset read the input's range (±5 for an input without one), and its `mode` unset is settled when the patch is built (`#[params(settles = …)]`): `blend` when its input is bounded and not a fixed voltage, `shift` otherwise, so it never switches while the patch plays. The DSL's `.erratic()` on a signal it does not know is ranged asks for `shift` across a nominal ±5.
- `.range(outMin, outMax, inMin?, inMax?)`: on a ranged collection the input range may be left out and comes from the engine; on any other it is required (a type error without it).
- `_round` becomes `$round`, `$floor`, `$ceil` and `_erratic` becomes `$erratic`, as ordinary chain modules; the `CollectionWithRange` overrides, `roundBound` and the DSL's bound values go.
- A module's result is ranged when its outputs are bounded by the rule above. The schema carries each output's kind and its tagged inputs; where boundedness depends on a value the types cannot see (`$erratic`'s `mode`, `$clamp`'s `min`/`max`, `$div`'s divisor), the generated typings return the unranged type and the run-time flag still applies.
- Out-lane controls read their source's bounds from the engine, sampled into the meter frames beside the live control values the panel already receives. The panel treats a point (`out({ level: 3 })`) like an unbounded source and spans its usual 0–10 or ±5.
- Where the engine bounds an output the DSL types as unranged (`$erratic` without a `mode`, whose bounded default is `blend`), the typings are conservative: `.range` needs its input range there.

## Changelog-visible changes

- `$remap` can leave out `inMin`/`inMax` to map from its input's own range (`New`: both were required).
- A ranged `.erratic()` defaults to `blend` where it defaulted to `clamp` (`Breaking`).
- `.add()`, `.mul()`, `.round()` and the other ruled modules keep a signal ranged, so two-argument `.range()` works after them (`New`).
- `$round`, `$floor`, `$ceil` and `$erratic` work as functions (`New`).
- Out-lane sliders on a derived source span its engine bounds.

## Work, one PR each

1. **Engine bounds, rules and engine consumers.** `Bounds`, the output kinds and tagged inputs, the static and demand passes, storage with runs, every write and read path, every rule above on the modules' current names, and `$remap`/`_erratic` reading their input's bounds. Tests through `get_bounds_at`: lock step on every read path with bounds that move every sample and bounds that step mid-block, a steady cycle across block boundaries, a mid-block swap with `set_initial_index`; each ruled module's `block_<module>.rs` checks its bounds paths agree like its samples, glides included; the static pass on cycles; storage only where read. A patch with no reader costs nothing.
2. **The DSL.** `$round`/`$floor`/`$ceil`/`$erratic` public; `.range` with an optional input range; the overrides and bound plumbing deleted; typings; the remaining changelog entries above.
3. **Out-lane controls** from engine bounds.
