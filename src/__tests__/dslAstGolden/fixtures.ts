/**
 * Patch sources for the golden harness that reach analyzer paths the
 * documented examples do not: template interpolation of `const` strings,
 * control groups, EQ panels, out options, and the source shapes (comments
 * inside callees, CRLF line endings) that position math must survive.
 */

const crlf = (source: string) => source.replace(/\n/g, '\r\n');

export const FIXTURES: Record<string, string> = {
    'interpolation-const': `const root = 'c4'
const third = 'e4'
const seq = $cycle($p(\`\${root} \${third} g4\`))
$sine(seq).out()
`,

    'interpolation-nested': `const a = 'c3 e3'
const b = \`\${a} g3\`
const c = \`<\${b} \${a}>\`
$saw($cycle($p(c))).out()
$saw($cycle($p(\`\${c} b3\`))).out()
`,

    'interpolation-unresolved': `const base = 'c4 e4'
let changing = 'g4'
const n = 3
$sine($cycle($p(\`\${base} \${changing} \${base}\`))).out()
$sine($cycle($p(\`\${'d4'} \${base}\`))).out()
$sine($cycle($p(\`\${n} \${base}\`))).out()
`,

    'interpolation-scale': `const key = 'c(maj)'
const degrees = '<0 2 4>*8'
const melody = $cycle($p.s(\`\${degrees} 7\`, key))
$sine(melody).out()
`,

    'interpolation-scoped-const': `const outer = 'c4'
function voice() {
    const inner = 'e4'
    return $sine($cycle($p(\`\${inner} \${outer}\`)))
}
voice().out()
`,

    'controls-grouped': `const synth = $cGroup('Synth')
const amp = $cGroup('Amp', { collapsed: true, group: synth })
const cutoff = $slider('cutoff', 800, 20, 8000, synth)
const level = $slider('level', 0.5, 0, 1, amp)
const drone = $toggleBtn('drone', false, synth)
$btn('kick', $cGroup('Fx'))
$saw('c2').amp(level).out()
`,

    'controls-units': `$sine($slider('pitch', 'a3', 'c2', 'c6')).out()
$sine($slider('freq', '440hz', '55hz', '880hz')).out()
$sine('c4').amp($slider('neg', -1, -2, 2)).out()
`,

    'controls-collapse-forms': `const open = $cGroup('Open', {})
const closed = $cGroup('Closed', { collapsed: false })
const other = $cGroup('Other', { group: open })
const computed = $cGroup('Computed', makeParams())
$slider('a', 1, 0, 2, open)
$slider('b', 1, 0, 2, closed)
$slider('c', 1, 0, 2, other)
`,

    'eq-shared': `const sig = $saw('c2')
$eq(sig, {
    sections: [{ type: 'lowShelf', freq: '150hz', gain: 3 }],
    ui: true,
}).out()
$eq(sig, { ui: true }).out()
`,

    'eq-per-channel': `$eq($saw(['c2', 'c3']), {
    sections: [[{ freq: 0 }], [{ freq: 1, type: 'peak', q: 2 }]],
    ui: true,
}).out()
$saw('c2').$.eq({ sections: [{ freq: -1 }], ui: true }).out()
`,

    'out-options': `$sine('c4').out()
$sine('e4').out({ mute: true })
$sine('g4').out({ solo: false, pan: -0.5 })
$sine('b4').outMono(0, { mute: false })
$setOutputGain(0.8)
`,

    'callee-shapes': `$tr808
    // the voice
    .kick('c1')
    .out()
$tr909.kick('c1').out()
$sine(/* pitch */ 'c4').out()
$unstable.filter.lp($saw('c2'), 'c5').out()
$saw('c2').$.lp('c4').$m.amp(0.5).out()
`,

    'config-keys': `$sine('c4', { 'phase': 0.25 }).out()
$saw('c2', { 1: 2, detune: +0.1 }).out()
$sine(~1).out()
$sine(-0.5e-3).out()
`,

    'crlf-lines': crlf(`const root = 'c4'
const g = $cGroup('G')
$sine($cycle($p(\`\${root} e4\`))).amp($slider('v', 0.5, 0, 1, g)).out({ mute: true })
$eq($saw('c2'), { sections: [{ freq: 0 }], ui: true }).out()
`),

    'unterminated-calls': `const s = $slider('s', 1, 0, 2
$sine('c4', { phase: 0.5
$eq($saw('c2'), { ui: true, sections: [{ freq: 0 }
`,
};
