/**
 * The TypeScript content mapper for patches, run as its own process by the
 * language server. A patch is JavaScript except for its track labels, whose
 * names JavaScript does not bind, so the mapper hands TypeScript the patch
 * verbatim followed by one declaration per track:
 *
 *     function __tracks(bass, lead) {
 *         return { bass: (<bass's expression>), lead: (<lead's expression>) };
 *     }
 *     var bass = __trackBinding(__tracks().bass)
 *
 * Each declared name is mapped onto the name inside its label, and each group
 * suffix onto a reference to its group's track, so hover, definition,
 * references and rename asked on a label reach the declaration and come back
 * as positions in the patch.
 */
import ts from 'typescript';
import { analyzeTrackLabels } from '../../shared/dsl/trackLabels';
import { JsonRpcConnection } from './jsonRpc';

const Feature = {
    Hover: 1,
    Definition: 8,
    TypeDefinition: 16,
    References: 64,
    DocumentHighlights: 128,
    Rename: 256,
    Formatting: 2048,
    SemanticTokens: 8192,
    SelectionRanges: 32768,
    DocumentSymbols: 262144,
    All: 1048575,
} as const;

/** What a declared track name answers for the label's name. */
const NAME =
    Feature.Hover |
    Feature.Definition |
    Feature.TypeDefinition |
    Feature.References |
    Feature.DocumentHighlights |
    Feature.Rename |
    Feature.DocumentSymbols;
/** What the label's own text keeps where a declaration answers for it. */
const LABEL_TEXT =
    Feature.Formatting | Feature.SemanticTokens | Feature.SelectionRanges;
const VERBATIM = 0;
const IGNORE = 0;

type Mapping = [
    virtualStart: number,
    virtualLength: number,
    originalStart: number,
    originalLength: number,
    kind: number,
    features?: number,
];

interface Span {
    start: number;
    end: number;
}

export function mapPatch(source: string) {
    const file = ts.createSourceFile(
        'patch.js',
        source,
        ts.ScriptTarget.Latest,
        true,
        ts.ScriptKind.JS,
    );
    const { tracks, errors } = analyzeTrackLabels(ts, file);
    let text = '';
    const mappings: Mapping[] = [];
    const verbatim = (
        start: number,
        end: number,
        features: number = Feature.All,
    ) => {
        if (end <= start) {
            return;
        }
        const length = end - start;
        mappings.push(
            features === Feature.All
                ? [text.length, length, start, length, VERBATIM]
                : [text.length, length, start, length, VERBATIM, features],
        );
        text += source.slice(start, end);
    };

    const names: Span[] = tracks.map(({ track, labelStart }) => {
        const start = labelStart + track.prefixLength;
        return { end: start + track.name.length, start };
    });
    const groups: Span[] = tracks
        .filter(
            ({ track }) =>
                track.group !== null &&
                tracks.some((other) => other.track.name === track.group),
        )
        .map(({ track, labelStart }) => {
            const start = labelStart + track.groupStart;
            return { end: start + track.group!.length, start };
        });

    // A label is no name of JavaScript's: only its formatting and coloring
    // read it here, and the declarations below answer for its names.
    let at = 0;
    for (const { labelStart, labelEnd } of tracks) {
        verbatim(at, labelStart);
        verbatim(labelStart, labelEnd, LABEL_TEXT);
        at = labelEnd;
    }
    verbatim(at, source.length);

    const tailStart = text.length;
    if (tracks.length > 0) {
        const parameters = tracks
            .map(({ track }) => `/** @type {TrackTap} */ ${track.name}`)
            .join(', ');
        const values = tracks
            .map(
                ({ track, expression }) =>
                    `${track.name}: (${source.slice(expression.getStart(file), expression.end)})`,
            )
            .join(',\n');
        text += `\n;function __tracks(${parameters}) { return {\n${values}\n}; }`;
        tracks.forEach(({ track }, i) => {
            text += '\n;var ';
            verbatim(names[i].start, names[i].end, NAME);
            text += ` = __trackBinding(__tracks().${track.name});`;
        });
        for (const group of groups) {
            text += '\n;';
            verbatim(group.start, group.end, NAME);
            text += ';';
        }
        // A track nothing reads is still a track.
        text += `\n;void [${tracks.map(({ track }) => track.name).join(', ')}];`;
        text += '\n;export {};';
    }

    return {
        diagnostics: errors.map((error) => ({
            code: 1,
            length: error.end - error.start,
            messageText: error.message,
            start: error.start,
        })),
        // The declarations repeat each track's expression, whose errors the
        // patch's own copy already reports.
        ...(text.length > tailStart
            ? {
                  diagnosticDirectives: {
                      directives: [[0, 0, tailStart, text.length, IGNORE]],
                      unusedExpectDirectiveDiagnostics: [],
                  },
              }
            : {}),
        extension: '.mjs',
        mappings,
        text,
    };
}

function run(): void {
    const connection = new JsonRpcConnection(process.stdin, process.stdout);
    connection.onRequest((method, params) => {
        switch (method) {
            case 'initialize':
                return {
                    diagnosticSource: 'shoreline',
                    positionEncoding: 'utf-16',
                };
            case 'openProject':
                return {};
            case 'transform':
                return mapPatch((params as { content: string }).content);
            default:
                return null;
        }
    });
}

run();
