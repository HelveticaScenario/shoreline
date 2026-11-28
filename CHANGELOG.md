# Changelog

## v0.5.0 — November 28, 2026

### Breaking

- `$mi.ringHat` is now part of `$mi.hiHat`: one trigger plays both hats, `_square` is the 808-style hat, `_ring` the ring-modulated one, and `mix` crossfades its default output from the first into the second, so you can sweep or accent between them. `$mi.hiHat` on its own sounds as before, but its `_output` is now `_square`.
  
  ```js
  // before
  hats: $mi.ringHat('a3', { trigger })
  closed: $mi.hiHat('a3', { trigger })._output
  // after
  hats: $mi.hiHat('a3', { trigger })._ring
  closed: $mi.hiHat('a3', { trigger })._square
  ```
- The `$unstable.filter.*` modules are removed. Use the filters that stay: `$lpf`, `$hpf`, `$bpf`, `$notch` and `$apf`, their `Sk`, `Ladder`, `Diode` and `Ota` variants such as `$lpfLadder` and `$lpfDiode`, `$multimode` and `$jup6f`. They take the sound, then the frequency, then `resonance` where they have one, so a call keeps its arguments, but the sound changes.
  
  ```js
  // before
  bass: $unstable.filter.lp($saw('c2'), $sine('0.1hz').range('c3', 'c6'), 3)
  // after
  bass: $lpf($saw('c2'), $sine('0.1hz').range('c3', 'c6'), 3)
  ```
- `$mi.phaseDistortion` no longer starts each new voice with a click: a new voice stays silent until its wave first crosses zero, which shaves a fraction of a cycle off its start. Set `waitForZeroCross` to `false` to hear the old start:
  
  ```js
  // before
  bass: $mi.phaseDistortion('c2')
  // after: sounding as before
  bass: $mi.phaseDistortion('c2', { waitForZeroCross: false })
  ```
- `$mi.sixOp` plays voices from DX7 `.syx` files: pass a file as its second argument, and add `#` and a voice name to pick one voice from a bank (`'/dx7/rom1a.syx#E.PIANO 1'`). Autocomplete lists every voice of every `.syx` file in your workspace. Plaits' 96 voices are built in, one file each under `builtin/dx7/plaits/`, and `bank` and `patch` are gone. It also plays up to `voices` notes at once, 2 unless you set it (up to 8), so each note's release rings on under the next; a note past the count fades out the oldest. Set `voices: 1` for one note at a time.
  
  ```js
  // before
  keys: $mi.sixOp(seq, { gate: seq, bank: 1, patch: 0 })
  // after
  keys: $mi.sixOp(seq, 'builtin/dx7/plaits/e-piano-1.syx', { gate: seq })
  ```
- The rest of the `$unstable` modules are gone too. `$unstable.reverb2` is now `$hall`, with the same sound and params, and is marked experimental, so its sound and params may still change. `$unstable.reverb` is removed: `$hall` takes the same params, so a call keeps its arguments, but the sound changes. The `$unstable.shape.*` modules are removed: use `$overdrive`, `$fold`, `$cheby` or `$softClip`, and retune `drive` or `amount` by ear, since their curves differ.
  
  ```js
  // before
  pad: $saw("c3").unstable.reverb2({ decay: 2 }).wet(2)
  lead: $saw("c4").unstable.shape.saturate("soft", 2)
  // after
  pad: $saw("c3").hall({ decay: 2 }).wet(2)
  lead: $saw("c4").overdrive(2)
  ```
- `$scopeXY` and `$v.scopeXY` draw one stereo signal: the left side moves the beam across and the right side moves it up, one trace per lane. A stereo wav holding a drawing in its two channels now plays straight into the scope. Join separate x and y signals with `$stereo`.
  
  ```js
  // before
  $scopeXY($sine('c3'), $sine('g3'), { lineWidth: 1.5 })
  // after
  $scopeXY($stereo($sine('c3'), $sine('g3')), { lineWidth: 1.5 })
  ```
- `$hz` is now `$hzToV`, naming the direction it converts beside the new `$vToHz`.
  
  ```js
  // before
  tone: $sine($hz(440))
  // after
  tone: $sine($hzToV(440))
  ```

### New

- `$lpf`, `$hpf` and `$bpf` have a second output, `._24db`: a steeper 4-pole version of the filter. Used on its own, each still gives its 2-pole sound, also available as `._12db`. A new `$notch` cuts a band instead: one null on `._12db`, two spread around the centre on `._24db`.
- Analog-modelled filters in four families, each saturating in its own way and self-oscillating at high resonance: Sallen-Key (`$lpfSk`, `$hpfSk`, `$bpfSk`, `$notchSk`), transistor ladder (`$lpfLadder`, `$hpfLadder`, `$bpfLadder`, `$notchLadder`, `$apfLadder`), diode ladder (`$lpfDiode`, `$hpfDiode`, `$bpfDiode`, `$notchDiode`) and OTA cascade (`$lpfOta`, `$hpfOta`, `$bpfOta`, `$notchOta`, `$apfOta`). Each has `._12db` and `._24db` outputs, the ladder, diode and OTA lowpasses and highpasses add `._6db` and `._18db`, and `drive` raises the level into the filter. On the Sallen-Key filters only the resonance clips, so a hot input swamps it for a wild, unruly sound.
- A brick-wall lowpass filter, `$lpfLeapfrog`, modelled on the North Coast Synthesis Leapfrog: a five-pole near-elliptic response with nulls at twice and three times the cutoff, a rippling passband that rings on sharp edges, and `resonance` that feeds it back through a clipper until it sings about a fourth below the cutoff.
- Wasp filters, after the EDP Wasp's logic-chip filter: `$lpfWasp`, `$hpfWasp`, `$bpfWasp`, `$notchWasp` and `$apfWasp`, each a 12 dB filter that turns dirty and erratic when fed a loud signal and whistles at full resonance without self-oscillating. They have no `drive` (the level you feed them is the drive) and pass no DC.
- The Docs panel has a Guide: chapters from your first sound through patterns, controls and mixing, ending in a cheat sheet, with examples to copy and your own keyboard shortcuts.
- New `$softClip` rounds off peaks above a knee so a signal never passes its `ceiling`, leaving quieter material untouched. The ceiling defaults to the audio output's full scale, so `$setEndOfChainCb((mix) => mix.softClip())` catches the whole mix before it clips at the output. `knee` sets how gradual the rounding is, from a hard clip at 0 to fully rounded at 5. `_tanh` rounds into the same ceiling along a gentler tanh curve, and saturates at every level when `knee` is 5.
- New `$vToHz` turns a pitch into its frequency in Hz, so `'200hz'` becomes 200. Feed it to `addHz` to detune by an amount set as a pitch, such as an Hz slider:
  
  ```js
  pitch.addHz($slider('offset', '1hz', '1hz', '1000hz').vToHz())
  ```

### Improved

- `$multimode` uses less CPU when only some of its outputs are patched, such as just its main output or `._lp`.
- A module with several outputs stays a bus or group when chained from `$bus()` or `$group()`, whichever output is read: `$bus().xover()._low` is a bus that tracks can send to. Its `.concat()` also takes a plain signal, which follows every output.
- Shoreline runs on Electron 45, bringing a newer Chromium with its security fixes.
- Path autocomplete in strings stays quick in workspaces with thousands of files: the list is read once and kept until files change, instead of being read from disk on every `/`.
- Code examples in the Docs panel read like the editor: hover a name to see its docs and see the same output hints. Go to Definition, or Cmd/Ctrl-click, on a module, global or type, in an example or your patch, opens its page in the Docs panel. Each example also has a copy button, so you can paste it straight into a patch.
- Format Document keeps a `.wet(...)` on the same line as the effect it mixes, so a long chain reads one effect per line, as in `.plate().wet(2)`.

### Fixed

- A param's default on its Docs panel card shows names such as `fall` (on `$slew`'s `rise`) as code, the way the editor's hover does, instead of with raw backticks.
- Deleting the file open in the editor moves to the neighbouring tab, the same as closing it.
- Clicking and then double-clicking a file in the explorer before it has loaded no longer opens it twice.
- Reopening the audio settings while devices are still being listed no longer resets the choices you made there.
- Scopes, `$v.preview` panels and inline EQ panels stay with their buffer when you switch to another tab and back, and the editor returns to where you left it.
- The docs examples for `$lpf` and `$hpf` no longer clip the output when you run them.
- `$mi.sixOp` no longer clicks at note-ons: fast attacks ramp in over half a millisecond as on Plaits, and a new note never restarts a voice that is still sounding, which clicked on voices whose oscillators restart at every note (DX7 key sync, as in `cs-80.syx`).

## v0.4.1 — November 22, 2026

### New

- On Linux, Shoreline can play through PipeWire and PulseAudio directly: pick them as the audio host in Audio Settings. With no host chosen it uses JACK when a JACK server is running, then PipeWire, then PulseAudio, then ALSA.

### Improved

- The error banner now shows the line a patch error comes from: syntax errors, errors thrown while the patch runs, and invalid module params, even when the patch has several modules of the same type.

### Fixed

- With JACK as the audio host, the output list no longer offers JACK's input, and the input list no longer offers its output.

## v0.4.0 — November 22, 2026

### Breaking

- A bus's audio can now be used only on the bus's own track, so any track can send to any bus from the mixer. Reading a bus anywhere else, by its name or through a `$bus()` kept in a `const`, is an error. The name still works in `sends`, and `.active`, `.level` and `.panning` still read the bus's mixer controls. To process a bus further, chain onto the bus track itself:
  
  ```js
  // before
  verb: $bus().plate()
  wet: $c(verb).hpf("c3")
  // after
  verb: $bus().plate().hpf("c3")
  ```
- `$wavetable` no longer turns the top harmonics of bright wavetables into harsh, out-of-tune tones (aliasing), and pitch sweeps now fade smoothly between brightness steps instead of jumping. Bright tables sound cleaner but a little less airy, most on high notes. There is no setting for the old sound, and patches need no changes.
- `$supersaw` holds a steadier sound: its saws start at phases spread so they never all line up, so the stack keeps a steady level instead of swelling as the detune drifts them; the left and right sides are centred on the same pitch, with an odd stack's undetuned saw in the centre; and raising `unison` crossfades to the bigger stack without a click. Patches with `$supersaw` sound slightly different; nothing in the patch needs to change.
- `$macro` is removed. Each of its engines has its own module: `vaVcf` → `$mi.vcf`, `phaseDistortion` → `$mi.phaseDistortion`, `sixOpA`/`B`/`C` → `$mi.sixOp`, `waveTerrain` → `$mi.waveTerrain`, `stringMachine` → `$mi.stringMachine`, `chiptune` → `$mi.chiptune`, `virtualAnalog` → `$mi.va`, `waveshaping` → `$mi.waveshaper`, `twoOpFm` → `$mi.twoOp`, `granularFormant` → `$mi.formant` or `$mi.zOsc`, `additive` → `$mi.additive`, `chords` → `$mi.chords`, `speech` → `$mi.speak` and the vowel modules, `swarm` → `$mi.swarm`, `filteredNoise` → `$mi.noise`, `particleNoise` → `$mi.particles`, `inharmonicString` → `$mi.string`, `modalResonator` → `$mi.modal`, `bassDrum` → `$mi.kick`, `snareDrum` → `$mi.snare`, `hiHat` → `$mi.hiHat`, and `wavetable` → `$wavetable`, whose built-in tables include `$macro`'s wavetables: pass a path like `'builtin/tables/mi/organ.wav'`. Where `$macro` gated the sound with `trigger`, add `$lpg` after the new module; for the `timbreAmt`, `morphAmt` and `fmAmt` sweeps, send `$perc` or `$adsr` to the param. Where you used `mix`, the replacements with two outputs take the same `mix`, crossfading their default `_blend` output from the main sound into the second one.
  
  ```js
  const seq = $`c3 eb3 g3 bb3`
  // before
  lead: $macro(seq, 'virtualAnalog', { trigger: seq, lpgDecay: 2 })
  // after
  lead: $mi.va(seq).lpg(seq, { decay: 2 })
  ```
- The trigger that restarts the cycle of `$sine`, `$saw`, `$pulse`, `$wavetable` and `$supersaw` is now `restart`, and their `sync` is internal hard sync, set in octaves: the wave runs that many octaves above its note and restarts with it, so sweeping `sync` gives sync tearing at the note's pitch. `restart` starts the cycle at the exact moment its trigger rises, so restarting from another oscillator stays in tune. Rename a trigger to `restart`, or move a sync oscillator's pitch into the note:
  
  ```js
  // before: restarted by each trigger, and a c3 stack synced to c2
  lead: $saw('c3', { sync: $clock._beatTrigger })
  keys: $supersaw('c3', { sync: $pulse('c2') })
  // after
  lead: $saw('c3', { restart: $clock._beatTrigger })
  keys: $supersaw('c2', { sync: 1 })
  ```
- `$unison`'s `spread` is now set like `$supersaw`'s `detune`: the semitones between the lowest and highest copy, 0.18 when unset, so an unset `spread` detunes the copies slightly instead of leaving them in tune. The old 0–10 scale put the outer copies `(spread / 10)²` octaves from the note; multiply that by 24 for the new value:
  
  ```js
  // before: outer copies a quarter of an octave out
  lead: $saw($unison('c3', 7, 5))
  // after: 6 semitones from lowest to highest
  lead: $saw($unison('c3', 7, 6))
  ```
- In a `.scale(...)` pattern, follow a degree with `+` or `-` marks to raise or lower it a semitone per mark, reaching notes outside the scale: `$("0 2 4 4+ 5").scale("c(major)")` plays G# as a passing note. The marks stay with the note through `.trans`, and in a just or Pythagorean scale they land on that tuning's notes. A `-` written straight after a number is now one of these marks, not a rest, so put a space before a rest that follows a number:
  
  ```js
  // before
  melody: $`0 2- 4`.saw()
  // after
  melody: $`0 2 - 4`.saw()
  ```
- `$chord` is removed: write the chord or scale as a string wherever a signal goes, and glide it through its inversions with `$inversion`. `$quantizer`'s `scale` takes the same strings, so its `root[chord]` and `chromatic` spec strings become `root:chord` and `c(chromatic)`.
  
  ```js
  // before
  pad: $saw($chord('C3[maj7]'))
  const c = $chord('C3[maj7]', { inversion: inv })
  lead: $sine($quantizer(lfo, $chord('A(minor)')))
  arp: $sine($quantizer(lfo, 'C[m7]'))
  // after
  pad: $saw('c3:M7')
  const c = $inversion('c3:M7', inv)
  lead: $sine($quantizer(lfo, 'a(minor)'))
  arp: $sine($quantizer(lfo, 'c:m7'))
  ```

### New

- The Output Device list has a System Default choice that follows your computer's default output, so unplugging headphones moves the sound to your speakers and the patch keeps playing. A device you pick by name stays put, even when it is also your computer's default and you plug in headphones. A banner says where the sound went. New installs start on it with no input, and No Input now stays off after a restart.
- The mixer shows a send dial per bus on every track and group, lettered like the buses. Turning one sets that track's send live and writes it into `.track({ sends })`, adding the send if it isn't there yet; letting go at 0 removes it. As with the other mixer controls, ⌃/⌘-drag edits the code only, and ⌃/⌘-right-click puts the code back to what is playing. A send level driven by a signal shows a locked dial that follows the signal, and the dials hide together when the mixer is too short to show them all. The pan knob draws the same way, its arc filling out from the centre.
- 25 wavetables from Mutable Instruments Plaits, Beads and Braids now ship with the app: pass a path like `'builtin/tables/mi/organ.wav'` to `$wavetable`, and the editor suggests them as you type `'builtin/'`. The Docs panel's new Built-ins page describes each table.
- New `$mi.va`, Plaits' virtual analog oscillator as its own module: a square and a saw a `detune` interval apart, with `square` and `shape` morphing each, and a `sync` output of two hard-synced oscillators for tearing sync leads. Its default `_blend` output crossfades `_dual` into `_sync` as `mix` rises.
- `$mi.noise` plays Mutable Instruments Plaits' filtered noise: noise held between ticks of a clock, from stepped crackle to smooth hiss, through a resonant filter you sweep from lowpass to highpass (`filtered`) or two bandpass filters (`bands`). Its `trigger` restarts the clock. Its default `_blend` output crossfades `_filtered` into `_bands` as `mix` rises.
- New `$mi.vcf`: Plaits' virtual analog VCF voice, a saw, square or pulse with a sub-octave square through a resonant filter that follows the pitch, with its `highpass` output beside the low-pass one. Its default `_blend` output crossfades the low-pass into the high-pass as `mix` rises.
- `$unison` has a `phase` output: feed it to an oscillator's `phaseOffset` and the copies never all line up, so the stack holds a steady level instead of swelling as they drift, e.g. `$saw(voices, { phaseOffset: voices._phase })`.
- New `$mi.chords`: Plaits' chord engine, eleven four-note chords on organ voices that turn into wavetable voices as `waveform` rises. `inversion` revoices the chord upward, and the `root` output plays its root alone. Its default `_blend` output crossfades the chord into `_root` as `mix` rises.
- New `$stereoFold` folds each note's `$unison` copies into one stereo lane, fanning them across left and right the way `$supersaw` spreads its saws, so any oscillator can make a wide unison stack: `$saw($unison('c3', 7, 3)).stereoFold(7)`. Its `width` sets how far the copies fan out.
- New `$mi.stringMachine`: Plaits' string machine, eleven four-note chords on string-machine organ voices through a low-pass filter and a stereo ensemble chorus, in one stereo lane. `tone` opens the filter and swirls the chorus in and out, and `registration` blends saw and square footages.
- New `$mi.chiptune`: Plaits' chiptune engine, chords of square waves over a stepped NES-style triangle bass on its own `bass` output. Patch `trigger` and it arpeggiates the chord instead, a note per trigger, in the direction and octave range `pattern` picks, with `decay` fading each note. Its default `_blend` output crossfades the chord into `_bass` as `mix` rises.
- `$mi` modules with two outputs now take a `mix` param and play a `_blend` output by default: as `mix` rises from 0 to 5, `_blend` crossfades from the main output into the second one. This covers `$mi.twoOp`, `$mi.waveshaper`, `$mi.waveTerrain`, `$mi.swarm`, `$mi.modal`, `$mi.string`, `$mi.particles`, `$mi.speak`, `$mi.formantVowel`, `$mi.samVowel` and `$mi.lpcVowel`. At the default `mix` of 0, `_blend` is the main output, so existing patches sound the same.
- New `$super` turns any voice into a detuned, stereo-spread unison stack like `$supersaw`'s: `$super('c3', (freq, { phaseOffset }) => $saw(freq, { phaseOffset }))`, or chained onto a signal or pattern like `$delay`: `` $`c3 e3 g3`.super((freq, { phaseOffset }) => $saw(freq, { phaseOffset })) `` stacks each note. Its `unison`, `detune` and `width` options set the copies per note, the semitones between the lowest and highest copy, and how far they fan out.
- New patches are saved as `.sl` files. Your existing `.mjs` and `.js` patches still open, run and save as before.
- Patterns play chords: write a note, a colon and a chord symbol, like `c3:M7`, `a:m9` or `g:7b9b13sus`, to play all its notes at once, and the same chord strings work anywhere a signal goes (`$saw('c3:M7')` is four voices). `^N` inverts a chord and a final `%` tunes it in just intonation. Chords are not allowed in `.scale` patterns.
- Patterns and signals play scales: a note with a scale in parentheses, like `d3(dorian)` or `c(0 2 4 7 9)`, plays every note of the scale at once, in a pattern or anywhere a signal goes (`$saw('d3(dorian)')` is seven voices). Scales are not allowed in `.scale` patterns.
- New `$inversion` glides any set of notes through their inversions: `amount` raises that many of the lowest notes an octave, crossfading the moving note between octaves so nothing clicks. Scale each voice by its `_level` output, as in `$saw(c).amp(c._level)`, and patch `gate` to invert only the notes whose gate is high, as `$quantizer` does; voices fade in and out as their gates change.
- New `$crossfade`: fades from `a` to `b` as `mix` rises from 0 to 5, for audio or control voltages alike. Modulate `mix` to morph between two oscillators, two LFOs or two melodies, or chain it as `sig.crossfade(other, mix)`.

### Improved

- A mixer meter turns red while its track clips, its levels in three shades of the theme's red. The peak readout above it holds the highest peak until you click it, and is red while that peak is above 0 dB.
- When you haven't chosen a sample rate or buffer size, Shoreline now runs at 48 kHz with a 256-frame buffer wherever your audio device supports them, instead of the device's own defaults.
- The Docs panel's Reference page is now called Modules, and the Globals page works like it: search, category filters, every example, and what each global returns. Hover docs for globals such as `$slider` and `$trackTap` now match the Globals page, and their links open it. `$delay` is listed on the Modules page.
- Examples in hovers, completions and the Reference and Docs panes, and the patch a new untitled file starts with, now use your **Quotes** formatter setting.
- ⌃/⌘-click a track's name in the mixer to jump the editor's cursor to its label, switching to the tab the mixer's tracks came from if another is showing, as with the controls in the Controls panel.
- The editor's "Add slider" quick fixes now offer `pan` on `$pan`, `offset` on `$quantizer`, `freq` and `rootFreq` on `$granular`, `$granularSampler` and `$clockedDelay`, `rate` on `$noise`, `min` and `max` on `$clamp`, and `rise` on `$slew`. Leaving those last four out still works as before.

### Fixed

- Changing the audio device keeps the patch playing, along with any recording and Ableton Link, when the new device runs at the same sample rate and buffer size. Changing the sample rate or buffer size stops playback, and a banner says so. Unplugging the output device you picked pauses the patch, and it picks up where it left off when you plug the device back in. If that device isn't connected, the sound plays on your default output until it is, and your choice and its settings are kept. If your input device is unplugged or changes its sample rate, Shoreline reopens it or tells you the input is off.
- Renaming a sample, image or video in the file explorer keeps the name you type instead of adding `.mjs` to it, and renaming a patch without an extension keeps its own extension.
- A `.scope()` on a pattern or other multi-output module now shows the output its own result is plugged into, and the module's default output otherwise (a pattern's pitch). `seq.scope()` beside `seq._gate.scope()` used to draw the gate twice; it now draws the pitch and the gate.
- Links (symlinks) in the workspace folder are followed everywhere: linked files and folders show in the file explorer and in path completions, update when their contents change, and samples, `$v.video` and `$v.image` files load through them, even when a link points outside the workspace (a sample library kept elsewhere, say). Files can be saved, moved and renamed into a linked folder, including one on another drive.

## v0.3.0 — November 20, 2026

### Breaking

- The output treats ±20 V as full scale, where it was ±40 V, so every patch plays 6 dB louder and a single voice at ±5 V sits at -12 dBFS. The meters read 0 dB at 20 V. To keep a patch at its old level, lower the main gain by half a step:
  
  ```js
  // before
  $setMainGain(5)
  // after
  $setMainGain(4.5)
  ```
- A send level in `.track({ sends: [bus, level] })` is now a linear mix amount, like `$amp`: 5 sends at full level, 2.5 at half and 0 not at all, instead of following the 12 dB-per-step `.gain()` curve. Patches with a send level between 0 and 5 send more than before, and a negative level inverts the send; to keep a send at about the same level, convert it from the gain scale:
  
  ```js
  // before: 4 on the gain scale, 12 dB down
  lead: $saw("c3").track({ sends: [verb, 4] })
  // after: a quarter of the signal
  lead: $saw("c3").track({ sends: [verb, 1.25] })
  ```

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
