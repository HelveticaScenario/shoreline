# Changelog

## v0.2.0 — November 18, 2026

### Breaking

- `$unstable.reverb`, `$unstable.reverb2`, `$dattorro`, `$plate`, `$reverb` and `$mi.chorus` take and give stereo lanes. Each side of the input feeds that side of the effect, and the output is one stereo lane instead of a left lane and a right lane. A pair of lanes is no longer read as left and right, so join them with `$stereo` first. Every lane of a chord now reaches both sides at its own level, so a chord drives the effect about 6 dB harder than before and no longer spreads across the sides: turn it down with `.amp()`, and spread it with `$pan` first.
  
  ```js
  // before
  const wet = $plate([$saw('c3'), $saw('g3')])
  $signal(wet[1]).outMono(1)
  $unstable.reverb($saw($chord('C3[m7]'))).out()
  // after
  const wet = $plate($stereo($saw('c3'), $saw('g3')))
  right$1m: $right(wet)
  pad: $pan($saw($chord('C3[m7]')).amp(2.5), 0, { spread: 5 })
      .unstable.reverb()
  ```
- `$sampler` and `$v.video(...).audio` play a two-channel file as one stereo lane instead of a left lane and a right lane, so `[1]` no longer picks the right side: use `$right(...)`. `$grains` plays both sides of a two-channel file in every voice, where each voice played one of its channels. Files with one channel, or more than two, play as before.
  
  ```js
  // before
  const drums = $sampler('/wavs/break.wav', $clock._beatTrigger)
  $signal(drums[1]).outMono(1)
  $signal($v.video('/clips/loop.mp4').audio[1]).outMono(2)
  // after
  const drums = $sampler('/wavs/break.wav', $clock._beatTrigger)
  beat$1m: $right(drums)
  soundtrack$2m: $right($v.video('/clips/loop.mp4').audio)
  ```
- `$granular` and `$granularSampler` give one stereo lane per lane, recording and playing each side, with `spread` scattering every grain across both. The `width` option and the `_voices` output are gone: the main output has a lane per voice, and `$pan` with `spread` fans the lanes out. `$granularSampler` plays a two-channel file in stereo on every lane. Each lane now carries its own dry signal, so a single input spread over several lanes (a chord on `freq`, say) passes its dry signal on every lane: lower `mix` or the level to taste.
  
  ```js
  // before: four voices mixed to stereo, fanned out by width
  $granular($saw($chord('C3[m7]')), $pulse('8hz'), { width: 4, spread: 2 }).out()
  // after: four stereo lanes, fanned out by $pan
  cloud: $pan($granular($saw($chord('C3[m7]')), $pulse('8hz'), { spread: 2 }), 0, { spread: 4 })
  
  // before: each voice on its own lane
  $granular($saw($chord('C3[m7]')), $pulse('8hz'))._voices
  // after: the main output
  $granular($saw($chord('C3[m7]')), $pulse('8hz'))
  ```
- `$stereoMix` is gone. `$pan` places each lane in the stereo field, with `spread` to fan a chord out, and a track sums the lanes into one stereo pair.
  
  ```js
  // before
  $stereoMix($saw($chord('C3[m7]')), { pan: -2, width: 4 }).out()
  // after
  chord: $pan($saw($chord('C3[m7]')), -2, { spread: 4 })
  ```
- `$enosc` plays in stereo: its default output `_all` puts the odd voices on the left and the even voices on the right, where it was a mono mix of every voice. `_odd` and `_even` are gone, so take the sides with `$left` and `$right`. For the mono mix at its old level, fold the output with `.mono()` and double it.
  
  ```js
  // before
  const e = $enosc('c2', $enosc.scale.harm4to7)
  e.out()
  e._odd.outMono(2)
  e._even.outMono(3)
  // after
  const e = $enosc('c2', $enosc.scale.harm4to7)
  mix: e.mono().amp(2)
  odd$2m: $left(e)
  even$3m: $right(e)
  ```
- `$ott` is gone. Build the same three-band squash from `$xover` and `$comp`, with `.pipeMix` standing in for `depth`:
  
  ```js
  // before
  $ott(drums, { depth: 4 }).out()
  // after
  const squash = { threshold: 1, ratio: 4, upwardThreshold: 0.5, upwardRatio: 4, attack: 0.003, release: 0.05, makeup: 1 }
  beat: drums
    .pipeMix((dry) => {
      const bands = $xover(dry, { lowMidFreq: '120hz', midHighFreq: '2500hz' })
      return $mix([bands._low, bands._mid, bands._high].map((band) => $comp(band, squash)))
    }, 4)
  ```
- `.send()` is gone. Send to a `$bus` with the `sends` option of `.track()`, which takes a bus track's name, a `[name, gain]` pair, or an array of them, so one track can feed several buses. A send carries the track's final signal, after its gain, pan, mute and solo:
  
  ```js
  // before
  $saw("c3").send(verb, 2).out();
  // after
  verb: $bus().plate()
  lead: $saw("c3").track({ sends: [verb, 2] })
  ```
- Tracks replace `.out()` and `.outMono()`. Put a label in front of a line to hear it in the mixer: the label names the track, `_name` mutes it, `S_name` solos it and `S__name` does both, `name$2` plays it on channels 2 and 3 and `name$2m` in mono on channel 2. `.track({ gain, pan, spread, sends })` sets how it mixes, and `.track().scope()` scopes the mixdown. The name then reads as the track's final signal anywhere in the patch (handy for sidechains). `$bus()` takes no callback: it is the mix of what tracks send to it, so chain effects on it and label the result, and other tracks send to that track's name. A bus track cannot be soloed, and keeps playing while other tracks are. The mixer's mute and solo buttons rewrite the prefix, and a soloed track plays even while muted; its fader and pan knob rewrite `.track()`, and bus tracks sit on the right, just left of the main strip. `$setOutputGain` is now `$setMainGain`, and `$trackTap` takes a track's name.
  
  ```js
  // before
  const verb = $bus((m) => m.plate().out({ label: "verb" }))
  $saw("c2").outMono(2, { label: "bass", mute: true })
  $sine("c4").send(verb, 2).out({ label: "lead", pan: -2 })
  $setOutputGain(3)
  // after
  verb: $bus().plate()
  _bass$2m: $saw("c2")
  lead: $sine("c4").track({ pan: -2, sends: [verb, 2] })
  $setMainGain(5.39)
  ```
- A stereo track plays a centred signal at its full level on both speakers, as a centred track does in a DAW, where a stereo `.out()` played it 3 dB lower. Patches written for the lower level play 3 dB louder; scale the signal down to keep the old level.
  
  ```js
  // before
  $saw('c3').out()
  // after, at the old level
  lead: $saw('c3').amp(3.54)
  ```
- The output treats ±40 V as full scale, where it was ±5 V, so a single voice at ±5 V plays at -18 dBFS and chords and mixes have room to sum. The meters read 0 dB at 40 V. `$setMainGain` defaults to 5, unity, so a patch that never set the main gain plays at the same level as before. One that set a main gain `g` sounds the same at `6.5 + 5 * log10(g / 5)`, so 4 becomes 6.02 and 2.5 becomes 5.
  
  ```js
  // before
  $setOutputGain(4)
  $saw('c3').outMono(1)
  // after, at the same level
  $setMainGain(6.02)
  lead$1m: $saw('c3')
  ```
- `.gain()`, `$gain`, `$mix`'s `gain` and a track's `gain` step 12 dB per unit: 5 is unity, 4.5 is -6 dB, 4 is -12 dB and 5.5 is +6 dB, fading to silence near 0. An old level `g` sounds the same at `5 + 5 * log10(g / 5)`, so 2.5 becomes 3.5 and 10 becomes 6.5.
  
  ```js
  // before
  $saw('c3').gain(2.5).outMono(1)
  // after, at the same level
  pad$1m: $saw('c3').gain(3.5)
  ```

### New

- Group tracks with `$group()`. A label ending in `$` and a group track's name plays into that group instead of the output, after its own pan, gain, mute and solo; the group is the track that starts from `$group()`, so chain effects on it. Groups nest, a group track can send to buses, and soloing a group plays it with all its tracks. A track's sends also pass through the gain, pan, mute and solo of every group it plays into, but not the groups' effects, so muting a group silences its tracks' sends too. The mixer shows a group with its tracks beside it and can fold them away.
  
  ```js
  kick$drums: $tr808.kick($`c ~ c ~`)
  hats$drums: $tr808.hiHat($`c*8`)
  drums: $group().comp().track({ gain: 4 })
  ```
- Patterns take `.playhead(...)` and `.ribbon(offset, length)`, the same as passing them to `.cycle({...})`. The settings carry through later pattern methods, and a later call replaces an earlier one.
  
  ```js
  melody: $`<c4 e4 g4 b4>`.ribbon(1, 2).saw()
  ```

### Improved

- Code in the release notes, both examples and short snippets, is colored the way the editor colors it.
- Writing a pattern as `$'c e g'` or `$"c e g"` now shows an error saying to use backticks (`` $`c e g` ``), with a quick fix that rewrites it.
- The quick fix that turns a value into a `$slider` now works on values inside lists and nested options, like one pitch in `$saw(['a3', 'c4'])`, a step of `$step`, a keyframe value or an `$eq` band's `gain`, and each slider gets that value's own range. On an empty `$phase.*` call such as `$phase.bend()`, it adds a slider for the amount.
- Audio settings keep the device list current on their own: plugging in or removing an interface shows up as soon as you reach for a device menu, so the Refresh Devices button is gone. Opening audio settings and starting up are faster on macOS and Windows, and Windows devices show their full names, such as "Speakers (Realtek Audio)".
- Font ligatures are now off by default in the editor and the performance window. To keep them, turn on "Font Ligatures" in the editor settings, or "Code Ligatures" in the performance settings.

### Fixed

- Saving a new untitled file or renaming an open one keeps the cursor and scroll position instead of jumping back to the top, even when saving over a file that is already open, and a new untitled file always opens at the top.
- When an edit turns a module's input stereo, the new right side carries on from the left instead of starting from silence, so a held envelope, a delay's echoes or `$granular`'s recording keep sounding on both sides without a dropout.
- In a narrow Reference pane, where the module list sits above the cards, the cards scroll again.
- The editor no longer shows a ghost `.mono()` after a mono signal that passes through `$g1`, `$g2` or `$g3`, `.lane()`, `.slice()`, `$cross`, a pattern's `.cycle()`, a `? :` or `??` choice, or a destructured `const`. `$trackTap`'s `level`, `panning` and `active` count as mono too.
- Calling `.scope()` on a pattern or other multi-output module now draws the output it feeds: `$perc($("0").scope())` shows the trigger `$perc` takes, not the pattern's pitch.
- A module call whose error a script catches with `try`/`catch` no longer leaves a broken module behind that stops the patch from running.
- The editor fonts that ship with the app now load. Picking one such as 0xProto, Hack or Iosevka used to show a fallback font unless it was installed on your computer. Recursive is now its monospaced Rec Mono Linear style, so code lines up in columns. A new install starts with the line cursor.
- Loud patches no longer saturate softly as they near full scale. The output plays every level below full scale untouched and clips only what goes past it.

## v0.1.0 — November 15, 2026

### Breaking

- A module's named outputs now start with an underscore: `seq.trig` is `seq._trig` and `$clock.beatTrigger` is `$clock._beatTrigger`.
  
  ```js
  // before
  $tr808.kick($cycle($p('c*4')).trig).out()
  // after
  $tr808.kick($cycle($`c*4`)._trig).out()
  ```
- A pattern or multi-output module plugged straight into an input now uses the output that fits it: its trigger in a trigger input, its pitch in a pitch input. Before, it always used its main output, so `$perc(seq)` read the notes' pitch as its trigger. Name the output to keep the old behavior.
  
  ```js
  const seq = $cycle($`c e g`)
  // before: the envelope read the pitch
  $saw('c3').amp($perc(seq)).out()
  // after: name the output to keep that
  $saw('c3').amp($perc(seq._cv)).out()
  ```
- `.$.` and `.$m.` are gone: chain a module directly, and follow it with `.wet(amount)` to blend it with the dry signal.
  
  ```js
  // before
  $saw('c3').$.lpf('800hz').out()
  $saw('c3').$m.lpf(2.5, '800hz').out()
  // after
  $saw('c3').lpf('800hz').out()
  $saw('c3').lpf('800hz').wet(2.5).out()
  ```
- `.mix(channels, mode)` is now `.setLanes(lanes, { mode })`. `$mix` is unchanged.
  
  ```js
  // before
  $c($sine('c3'), $sine('e3')).mix(1, 'average').out()
  // after
  $c($sine('c3'), $sine('e3')).setLanes(1, { mode: 'average' }).out()
  ```
- Scale patterns are now made with `.scale(...)` on any pattern, and `$p.s` is gone. Transpose them with `.trans`, which works after `.fast`, `.slow`, `.struct`, `.beat` and `.glide`, and on a `$arrange` of scale patterns, where each section moves within its own scale. `.scale` on a scale pattern changes its scale. `$p.s(...).add(...)` becomes `.trans(...)`, and `$p.s(...).sub(...)` becomes `.trans(...)` with negative degrees. `.add` and `.sub` on a scale pattern act on its signal instead.
  
  ```js
  // before
  $sine($p.s('0 2 4', 'c(maj)').add('0 1')).out()
  // after
  $sine($`0 2 4`.scale('c(maj)').trans('0 1')).out()
  ```
- `$clock.playhead` is a single lane counting bars (2.5 is halfway through the third bar), and stays sample-accurate however long a song runs. It used to be two lanes: the position within the bar and the count of bars. An onset that falls exactly on a sample may now fire one sample later.
  
  ```js
  // before: position within the bar
  $saw('c3').amp($clock.playhead[0]).out()
  // after
  $saw('c3').amp($clock._playhead.wrap(0, 1)).out()
  ```
- Files load by their path in the workspace, starting with `/`. `$wavs()` is gone, so a wav can live anywhere in the workspace, and `$v.image` and `$v.video` paths gain a leading `/`.
  
  ```js
  // before
  $sampler($wavs().kick, $clock.beatTrigger).out()
  $v.image('pictures/photo.png').out()
  // after
  $sampler('/wavs/kick.wav', $clock._beatTrigger).out()
  $v.image('/pictures/photo.png').out()
  ```
- `$fold` follows the 4ms Ensemble Oscillator's fold, which is gentler at low and middle `amount` settings. Raise `amount` to get close to the old sound.
  
  ```js
  // before
  $fold($saw('c2'), 2.5).out()
  // after: about the same fold
  $fold($saw('c2'), 3.7).out()
  ```
- `$lpg` `decay` settings below 0.5 now give shorter plucks; before, only the pluck's tail got shorter below 0.5. Set `decay` to 0.5 for a pluck close to the old one.
  
  ```js
  // before
  $lpg($saw('c3'), $pulse('4hz'), { decay: 0.2 }).out()
  // after: close to the old pluck
  $lpg($saw('c3'), $pulse('4hz'), { decay: 0.5 }).out()
  ```
- `$scaleAndShift`, `.amplitude()` and `.shift()` are gone. Use `.amp()` to scale and `.add()` to offset; `$amp`, `$gain`, `$add`, `$sub`, `$mul`, `$div` and `$exp` also work as functions, like any other module.
  
  ```js
  // before
  $scaleAndShift(lfo, 2.5, 1)
  $saw('c3').amplitude(2.5).out()
  lfo.shift(1)
  // after
  lfo.amp(2.5).add(1)
  $saw('c3').amp(2.5).out()
  lfo.add(1)
  ```
- `$ott`'s `depth` holds at the ends of its 0–5 range: past 5 it stays fully compressed instead of overshooting louder with the dry signal inverted, and below 0 it stays dry.
  
  ```js
  // before: depth past 5 pushed beyond the compressed signal
  $ott(drums, { depth: 8 }).out()
  // after: the same as depth 5
  $ott(drums, { depth: 5 }).out()
  ```
- `.exp()` holds an exponent below 0 at 0, so its output only ever rises with its input. For the old reciprocal powers, divide 1 by the positive power.
  
  ```js
  // before
  lfo.exp(-2)
  // after
  $div(1, lfo.exp(2))
  ```
- `.erratic()` on a ranged signal blends toward random values within its range by default, instead of shifting and clamping. Pass `mode: 'clamp'` to keep the old movement. With `mode: 'shift'` the result can leave the range, so it no longer has one: give `.range()` the input range too.
  
  ```js
  // before
  $sine('0.2hz').range(0, 5).erratic(2)
  // after
  $sine('0.2hz').range(0, 5).erratic(2, { mode: 'clamp' })
  
  // before
  $sine('0.2hz').range(0, 5).erratic(2, { mode: 'shift' }).range(0, 1)
  // after
  $sine('0.2hz').range(0, 5).erratic(2, { mode: 'shift' }).range(0, 1, 0, 5)
  ```
- The docs and messages call the elements of a polyphonic signal lanes, and the names follow. `$midiCV`'s `channels` is `lanes`, `$supersaw`'s `voices` is `unison`, `$outLane` is `$trackTap`, and `$track` is `$keyframes`.
  
  ```js
  // before
  const midi = $midiCV({ channels: 4 })
  $supersaw('c3', { voices: 7 }).out()
  $signal($outLane('bass').level)
  $track([[0, 0], [5, 1]])
  // after
  const midi = $midiCV({ lanes: 4 })
  $supersaw('c3', { unison: 7 }).out()
  $signal($trackTap('bass').level)
  $keyframes([[0, 0], [5, 1]])
  ```
- `$supersaw` makes one stereo lane per note, its `unison` saws spread from left to right, instead of one lane per saw. A chord stays one lane per note, and filters after it run on each side.
  
  ```js
  // before: seven lanes, spread by the out
  $supersaw('a3', { voices: 7 }).out({ width: 5 })
  // after: one stereo lane
  $supersaw('a3', { unison: 7 }).out()
  ```
- `.out()` plays stereo lanes in stereo, and its `width` option is now `spread`, matching `$pan`. `$input` gives stereo lanes, pairing input channels 1 and 2, 3 and 4, and so on; `$inputMono` gives one lane per channel as `$input` did. `$trackTap` gives one lane, stereo for a stereo out, and its `pan` is `panning`.
  
  ```js
  // before
  $saw($chord('C3[m7]')).out({ width: 5 })
  $input[1].outMono(0)
  $signal($outLane('bass').pan)
  // after
  $saw($chord('C3[m7]')).out({ spread: 5 })
  $inputMono[1].outMono(0)
  $signal($trackTap('bass').panning)
  ```
- `$p` is now `$`, and also works as a template tag, so a pattern can be written `` $`c e g` ``. `$p.arrange` is now `$arrange`.
  
  ```js
  // before
  $saw($p('c e g')).out()
  $p.arrange([2, $p('c')], [1, $p('e')]).saw().out()
  // after
  $saw($`c e g`).out()
  $arrange([2, $`c`], [1, $`e`]).saw().out()
  ```
- `$delay` and `.delay()` return only the delayed signal, without a `buffer` to read extra taps from. For more echo times, give `time` several values: each becomes its own feedback delay. For plain taps without feedback, read a `$buffer` with `$delayRead` at several times, as in `$delayRead($buffer(src, 1), [0.25, 0.5])`.
  
  ```js
  // before
  const echo = $delay(src, 0.25)
  echo.out()
  $delayRead(echo.buffer, 0.5).out()
  // after
  $delay(src, [0.25, 0.5]).out()
  ```

### New

- Release notes now appear in the app. Before you update, you can read everything that changed since your version, with any changes that might affect your patches listed first. After an update, a "What's new" window shows what changed, and Help → Release Notes shows the full history at any time.
- Experimental modules are now labeled in the editor: hover docs, autocomplete and the Reference pane mark them, so you know their sound and parameters may still change between releases. `$comp` and `$xover` are experimental.
- The engines of Mutable Instruments Plaits arrive as `$mi.*` modules, each knob a patchable input. Oscillators: `$mi.twoOp`, `$mi.sixOp` (DX7-style FM with patch banks), `$mi.formant`, `$mi.zOsc`, `$mi.waveshaper`, `$mi.phaseDistortion`, `$mi.additive`, `$mi.waveTerrain`, `$mi.swarm`, `$mi.divideDown`. Struck and plucked: `$mi.modal`, `$mi.string`, `$mi.particles`. Drums: `$mi.kick`, `$mi.synthKick`, `$mi.snare`, `$mi.synthSnare`, `$mi.hiHat`, `$mi.ringHat`. Speech: `$mi.formantVowel`, `$mi.samVowel`, `$mi.lpcVowel`, `$mi.speak`. And `$mi.chorus`, a stereo ensemble. Patch `$lpg` after one for Plaits' plucked sound. The drums, `$mi.modal`, `$mi.string`, `$mi.sixOp` and `$mi.speak` take an `accent` that works like it does on the `$tr808` and `$tr909` drums: 0 is an ordinary hit and 5 up to twice as loud, read as each hit is struck.
- `$enosc` plays a bank of sine voices tuned to a scale, after the 4ms Ensemble Oscillator. The voices sit on octaves and fifths unless you pick a scale from `$enosc.scale` (or write your own with `$enosc.scale.custom`). Shape the voices with `spread`, `detune`, `crossFm`, `fold`, `cheby` and `segment`, and split them across the stereo field with outputs such as `_odd` and `_even`.
- `$chord` has an `inversion` control that raises its lowest notes an octave (or drops its highest, for negative values), gliding smoothly between voicings. A new `_level` output gives each voice's loudness.
- A pattern works like any other signal: chain modules and methods straight off it, scope it, or send it out, and every use of the same pattern shares one sequencer.
  
  ```js
  const seq = $`c3 e3 g3`
  seq.saw().amp($perc(seq)).out()
  seq.scope()
  ```
  
  When a pattern or multi-output module is plugged into an input, the editor shows which of its outputs that input takes; double-click the hint to write it into your code.
- `$wavMeta('/wavs/loop.wav')` reads a wav's tempo, loops and cue points, so a patch can follow a loop's own tempo: `$setTempo($wavMeta('/wavs/loop.wav').bpm ?? 120)`.
- Typing a path that starts with `/` completes the wavs, images and videos in your workspace, and dragging a file from the explorer into the editor writes its path. The explorer lists images and videos too.
- New quick fixes: add a control for every param a module call leaves unset, turn a `$slider` back into `$c`, and extract a value to a constant or function, or inline a variable. Wrapping a single-value `$c` or `$signal` in a control now replaces the whole call.
- `$remap` can leave out `inMin` and `inMax` to map from its input's own range: `$remap($adsr(gate), 0, 1)` maps the envelope's 0–5 onto 0–1. An input without a range maps from -5–5.
- `.range(outMin, outMax)` works after any chain that keeps a signal's range, such as `.add()`, `.mul()`, `.div()` by a number, `.clamp()`, `.slew()`, `.round()` or `.erratic()`, mapping from the range that results. `$signal` takes `min` and `max` to give a value a range of its own:
  
  ```js
  $sine('1hz').range(0, 2).add(1).range(-5, 5)
  $signal(3, { min: 0, max: 10 }).range(0, 1)
  ```
- `$round`, `$floor` and `$ceil` are modules of their own, so you can call them directly, like `$floor($saw('0.5hz').range(0, 4))`, as well as through `.round()`, `.floor()` and `.ceil()`. `.wet()` after them blends the rounded signal with the original.
- `$erratic` is a module of its own, so you can call it directly, like `$erratic($sine('2hz'), 1)`, as well as through `.erratic()`, and `.wet()` after it blends the varied signal with the original. Its `min`, `max` and `mode` now work on any signal: `'clamp'` and `'blend'` keep a signal without a range of its own within -5 to 5, or within `min` and `max`.
- A lane can carry a stereo pair. `$stereo(left, right)` joins two signals into one, `$left`, `$right` and `$mono` take a side or the average back out, and `$mid`, `$side` and `$fromMidSide` work in mid/side. `$pan` places each lane in the stereo field, with `spread` to fan a chord out. Filters, oscillators, envelopes, `$buffer` delays and the other modules that work lane by lane run on each side, and a stereo signal on a param such as `$eq`'s band frequency or `$wowFlutter`'s `speed` gives each side its own. Scopes draw both sides of a stereo lane, and the editor shows a ghost `.mono()` wherever a stereo signal reaches an input that folds it to mono; accept it to write the fold into the code.
  
  ```js
  const pair = $stereo($saw('c3'), $saw('g3')).lpf('c6')
  $fromMidSide($mid(pair), $side(pair).mul(2)).out()
  $pan($saw($chord('C3[m9]')), 0, { spread: 4 }).out()
  ```
- Every signal has a `.concat(other)` method that joins it with another of the same type, lane after lane. Modules with several outputs join output by output, so `$enosc('c3').concat($enosc('g3'))._all` works and the joined result still plugs in as the output that fits; patterns join the same way.
- Choose how Format Document quotes strings under **Quotes** in the Formatter settings (or `prettier.quotes` in your config): single, double or backtick. Backtick keeps quotes only where JavaScript requires them, such as import paths and object keys; single and double turn template literals with no `${}`, line break or tag into quoted strings. New installs start with backticks; existing setups keep their quotes. Controls such as `$slider` and `$btn` also accept backtick labels and values like `` $btn(`Kick`) ``.

### Improved

- Settings open in the sidebar, which can sit on the left or the right; click the active sidebar button to close it. If you rebound keys with the `inSettingsModal` condition, it is now `settingsVisible`.
- A bottom bar opens the mixer and the docs in a resizable panel under the editor. The command `shoreline.toggleVuMeters` is now `shoreline.toggleMixer`.
- The module reference has a list of module names to jump between and category filters, and shows whether each output is a gate, a trigger, or a signal with its voltage range.
- Patches use less CPU, especially with many scopes or meters, and sounds that fade to silence no longer cost extra CPU on Intel and AMD computers.
- `$macro` is deprecated and will be removed in a later release; its docs name the `$mi.*` module that replaces each of its engines.
- Changing a `.wet()` or `.pipeMix()` amount, or `$ott`'s `depth`, while a patch plays glides to the new blend instead of jumping to it.
- Changing the amount in `.add()`, `.sub()`, `.mul()`, `.div()` or `.exp()` while a patch plays glides to the new value instead of jumping to it.

### Fixed

- `$trackTap(...).level` follows an out whose gain is a `$deferred()`.
- The mixer's mute, solo, pan and gain edit `.out()` calls correctly when the options use shorthand such as `{ mute }` or a ternary; mute no longer adds a second `mute: true`.
- Sliders, quick fixes and step highlights work on `$tr808` drums, on quoted option keys such as `{ 'phase': 0.5 }`, and on constants declared inside functions.
- An `$eq` edited from its panel inside a feedback loop stays in step with the loop.
- A huge power in `$exp`, such as 5 V to the 60th, holds at the largest value a signal can carry instead of dropping to silence at its peaks.

## v0.0.142

Releases up to and including 0.0.142 have no release notes.
