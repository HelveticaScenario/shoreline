/**
 * Fields of the option object taken by `.track()`. The generated DSL lib
 * declares the interface from these and lists them in the method's own
 * JSDoc, and the Docs panel renders the same fields.
 */

export interface TrackOptionField {
    name: string;
    type: string;
    doc: string;
}

export const TRACK_OPTIONS: readonly TrackOptionField[] = [
    {
        doc: 'Gain on the `.gain()` scale (5 = unity, 12 dB per step of 1), applied after the mixdown',
        name: 'gain',
        type: 'Poly<Signal>',
    },
    {
        doc: 'Pan position (-5 = left, 0 = center, +5 = right). Default 0. Stereo tracks only',
        name: 'pan',
        type: 'Poly<Signal>',
    },
    {
        doc: 'How far the lanes fan out across the stereo field (0 = none, 5 = edge to edge). Default 0. Stereo tracks only',
        name: 'spread',
        type: 'LaneSum<Signal>',
    },
    {
        doc: "Buses this track also feeds with its final signal, after its gain, pan, mute and solo and those of each group it plays into: a bus track's name, a `[name, gain]` pair, or an array of them",
        name: 'sends',
        type: 'Send | Send[]',
    },
];

/** The fields a bus's own track takes: a bus cannot send to another bus. */
export const BUS_TRACK_OPTIONS: readonly TrackOptionField[] =
    TRACK_OPTIONS.filter((f) => f.name !== 'sends');

/** One-line type expression, e.g. `interface { gain?: Poly<Signal>; ... }`. */
export function trackOptionsDefinition(
    fields: readonly TrackOptionField[],
): string {
    return `interface { ${fields.map((f) => `${f.name}?: ${f.type};`).join(' ')} }`;
}

/** Interface members with JSDoc, indented for the generated lib. */
export function renderTrackOptionsMembers(
    fields: readonly TrackOptionField[],
): string {
    return fields
        .map((f) => `  /** ${f.doc} */\n  ${f.name}?: ${f.type};`)
        .join('\n');
}

/**
 * Markdown list of the fields as JSDoc body lines, for a method whose hover
 * should show the options without following the interface.
 */
export function renderTrackOptionsJsDocList(
    fields: readonly TrackOptionField[],
): string {
    return fields
        .map((f) => `   * - \`${f.name}\` (\`${f.type}\`): ${f.doc}`)
        .join('\n');
}
