import {
    VIDEO_CHAIN,
    VIDEO_CHAIN_EXAMPLES,
    VIDEO_DOCS,
    VIDEO_INTRO,
    type VideoChainDoc,
    type VideoDocParam,
} from '../../shared/dsl/videoDocs';

const WRAP_WIDTH = 76;

/** Breaks `text` into lines of at most `width` characters, keeping paragraph breaks. */
function wrapText(text: string, width: number): string[] {
    const lines: string[] = [];
    text.split('\n\n').forEach((paragraph, i) => {
        if (i > 0) lines.push('');
        let line = '';
        for (const word of paragraph.split(' ')) {
            if (line !== '' && line.length + 1 + word.length > width) {
                lines.push(line);
                line = word;
            } else {
                line = line === '' ? word : `${line} ${word}`;
            }
        }
        lines.push(line);
    });
    return lines;
}

function jsdoc(
    indent: string,
    description: string,
    params: VideoDocParam[],
    examples: string[],
): string {
    const width = WRAP_WIDTH - indent.length;
    const lines = wrapText(description, width);
    for (const { name, description: text } of params) {
        const [first, ...rest] = wrapText(`@param ${name} ${text}`, width);
        lines.push(first, ...rest.map((line) => `  ${line}`));
    }
    for (const example of examples) {
        lines.push(...`@example ${example}`.split('\n'));
    }
    return [
        `${indent}/**`,
        ...lines.map((line) =>
            line === '' ? `${indent} *` : `${indent} * ${line}`,
        ),
        `${indent} */`,
    ].join('\n');
}

/** The `declare const $v` block with JSDoc, generated from {@link VIDEO_DOCS}. */
export function generateVideoNamespace(): string {
    const members = VIDEO_DOCS.map((doc) => {
        const declarations = doc.declarations
            .map((declaration) =>
                declaration
                    .split('\n')
                    .map((line) => `    ${line}`)
                    .join('\n'),
            )
            .join('\n');
        return `${jsdoc('    ', doc.description, doc.params, doc.examples)}\n${declarations}`;
    });
    return [
        jsdoc(
            '',
            VIDEO_INTRO.description,
            [],
            [...VIDEO_INTRO.examples, ...VIDEO_CHAIN_EXAMPLES],
        ),
        'declare const $v: {',
        ...members,
        '};',
    ].join('\n');
}

function renderMembers(entries: VideoChainDoc[]): string {
    return entries
        .flatMap(({ description, declarations, params }) => [
            jsdoc('    ', description, params ?? [], []),
            ...declarations.map((declaration) => `    ${declaration}`),
        ])
        .join('\n');
}

/**
 * The members a video signal has besides its type tag and channels: `pipe`,
 * `pipeMix` and the methods of {@link VIDEO_CHAIN}.
 */
export function generateVideoChainMembers(): string {
    return [
        '    /** Calls `fn` with this signal and returns what it returns. */',
        '    pipe<U>(fn: (self: VideoSignal) => U): U;',
        '    /** Calls `fn(this, element)` for each element of `array` and returns the results. */',
        '    pipe<U, E>(fn: (self: VideoSignal, element: E) => U, array: E[]): U[];',
        '    /** Crossfades this signal against `fn(this)`: 0 is this signal, 1 the result (default 0.5). */',
        '    pipeMix(fn: (self: VideoSignal) => VideoValue, mix?: VideoValue): VideoSignal;',
        renderMembers(VIDEO_CHAIN),
    ].join('\n');
}
