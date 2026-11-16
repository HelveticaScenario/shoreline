/**
 * Track labels: a top-level labelled statement (`bass$2: $saw('c2')`) sends
 * its signal to the mixer as a track. The label names the track, sets its
 * mute or solo state and its output channels, and binds the track's name for
 * the rest of the patch. The executor, the editor's TypeScript worker and the
 * mixer's source edits all read labels through this module, so they agree.
 */
import type * as TS from 'typescript';

/** A track's mute/solo state, as its label prefix writes it. */
export type TrackState = 'mute' | 'solo' | null;

export interface TrackLabel {
    state: TrackState;
    /** The label without its prefix and channel suffix; the bound name. */
    name: string;
    /** Length of the mute/solo prefix, where the name starts in the label. */
    prefixLength: number;
    /** The name as the mixer shows it: underscores read as spaces. */
    displayName: string;
    /** Stereo base channel, or the mono channel. */
    channel: number;
    mono: boolean;
    /** The group track this track plays into instead of output channels. */
    group: string | null;
    /** Offset in the label where the suffix's group name starts. */
    groupStart: number;
}

/** Highest stereo base channel: the right side plays on `channel + 1`. */
const MAX_STEREO_CHANNEL = 62;
const MAX_MONO_CHANNEL = 63;

const FORMAT_HINT =
    "Track labels are name, _name (muted) or S_name (soloed), optionally followed by $channel, $channelm for a mono track, or $group for a group track's name, e.g. bass$2 or kick$drums";

/** Prefix length and state of `text`'s mute/solo prefix. */
function splitPrefix(text: string): [length: number, state: TrackState] {
    if (/^[sS]_/.test(text)) {
        return [2, 'solo'];
    }
    if (text.startsWith('_')) {
        return [1, 'mute'];
    }
    return [0, null];
}

/** A track name: an identifier without `$`, which starts the suffix. */
const IDENTIFIER = /^\p{ID_Start}[\p{ID_Continue}‌‍]*$/u;

/**
 * Names a label may parse to but the patch can never read as its binding: the
 * patch runs as a strict function body, which binds `arguments` itself and
 * reserves the rest.
 */
const UNBINDABLE = new Set([
    'arguments',
    'await',
    'eval',
    'implements',
    'interface',
    'let',
    'package',
    'private',
    'protected',
    'public',
    'static',
    'yield',
]);

/** The track a label declares, or why the label declares none. */
export function parseTrackLabel(text: string): TrackLabel | { error: string } {
    const [prefixLength, state] = splitPrefix(text);
    const rest = text.slice(prefixLength);
    const dollar = rest.indexOf('$');
    const name = dollar === -1 ? rest : rest.slice(0, dollar);
    const suffixText = dollar === -1 ? null : rest.slice(dollar + 1);
    const suffix = suffixText === null ? null : /^(\d+)(m?)$/.exec(suffixText);
    const group = suffixText !== null && !suffix ? suffixText : null;
    if (
        name.length === 0 ||
        name.startsWith('_') ||
        !IDENTIFIER.test(name) ||
        (group !== null && !IDENTIFIER.test(group))
    ) {
        return { error: `\`${text}\` is not a track label. ${FORMAT_HINT}` };
    }
    // A name that reads as a solo prefix would become another track once the
    // mixer clears its own prefix.
    if (/^[sS]_/.test(name)) {
        return {
            error: `\`${text}\`: a track name cannot start with s_ or S_, which marks a soloed track`,
        };
    }
    if (UNBINDABLE.has(name)) {
        return {
            error: `\`${text}\`: ${name} cannot name a track, since a patch cannot read it as one`,
        };
    }
    const mono = suffix?.[2] === 'm';
    const channel = suffix ? Number(suffix[1]) : 0;
    const max = mono ? MAX_MONO_CHANNEL : MAX_STEREO_CHANNEL;
    if (channel > max) {
        return {
            error: `\`${text}\`: a ${mono ? 'mono' : 'stereo'} track's channel must be 0-${max}, got ${channel}`,
        };
    }
    return {
        channel,
        displayName: name.replace(/_/g, ' '),
        group,
        groupStart: prefixLength + dollar + 1,
        mono,
        name,
        prefixLength,
        state,
    };
}

/** `text` with its mute/solo prefix replaced by the one `state` writes. */
export function withTrackState(text: string, state: TrackState): string {
    const [prefixLength] = splitPrefix(text);
    const prefix = state === 'mute' ? '_' : state === 'solo' ? 'S_' : '';
    return prefix + text.slice(prefixLength);
}

/** A name as `$trackTap` matches it: spaces and underscores alike. */
export function trackNameKey(name: string): string {
    return name.replace(/ /g, '_');
}

export interface TrackStatement {
    track: TrackLabel;
    statement: TS.LabeledStatement;
    /** The labelled expression. */
    expression: TS.Expression;
    /** Offset of the label's first character. */
    labelStart: number;
    /** Offset just past the label's identifier. */
    labelEnd: number;
    /** Offset just past the colon. */
    colonEnd: number;
}

export interface TrackLabelError {
    start: number;
    end: number;
    message: string;
}

export interface TrackLabelAnalysis {
    tracks: TrackStatement[];
    errors: TrackLabelError[];
}

/**
 * Names the patch's own scope binds: its top-level declarations and every
 * `var` outside a function, which hoists to the top.
 */
function topLevelNames(ts: typeof TS, file: TS.SourceFile): Set<string> {
    const names = new Set<string>();
    const bind = (name: TS.BindingName): void => {
        if (ts.isIdentifier(name)) {
            names.add(name.text);
            return;
        }
        for (const element of name.elements) {
            if (!ts.isOmittedExpression(element)) {
                bind(element.name);
            }
        }
    };
    for (const statement of file.statements) {
        if (
            (ts.isFunctionDeclaration(statement) ||
                ts.isClassDeclaration(statement)) &&
            statement.name
        ) {
            names.add(statement.name.text);
        }
    }
    const visit = (node: TS.Node): void => {
        if (ts.isFunctionLike(node) || ts.isClassLike(node)) {
            return;
        }
        if (
            ts.isVariableDeclarationList(node) &&
            (node.parent === undefined ||
                node.parent.parent === file ||
                (node.flags & ts.NodeFlags.BlockScoped) === 0)
        ) {
            for (const declaration of node.declarations) {
                bind(declaration.name);
            }
        }
        ts.forEachChild(node, visit);
    };
    ts.forEachChild(file, visit);
    return names;
}

/**
 * Every track `file` declares, and every label that breaks the rules: one
 * that does not parse as a track label, sits below the top level, labels
 * anything but an expression, or repeats a name or a declaration's.
 */
export function analyzeTrackLabels(
    ts: typeof TS,
    file: TS.SourceFile,
): TrackLabelAnalysis {
    const tracks: TrackStatement[] = [];
    const errors: TrackLabelError[] = [];
    const declared = topLevelNames(ts, file);
    const seen = new Set<string>();
    const visit = (node: TS.Node): void => {
        if (ts.isLabeledStatement(node)) {
            const label = node.label;
            const start = label.getStart(file);
            const end = label.end;
            const fail = (message: string) =>
                errors.push({ end, message, start });
            const parsed = parseTrackLabel(label.text);
            if (node.parent !== file) {
                fail(
                    `\`${label.text}\`: labels declare tracks, so they only go at the top level of a patch`,
                );
            } else if ('error' in parsed) {
                fail(parsed.error);
            } else if (!ts.isExpressionStatement(node.statement)) {
                fail(
                    `\`${label.text}\`: a track label goes before a signal or a $bus`,
                );
            } else if (
                ts.isBinaryExpression(node.statement.expression) &&
                node.statement.expression.operatorToken.kind ===
                    ts.SyntaxKind.CommaToken
            ) {
                fail(`\`${label.text}\`: a track takes one expression`);
            } else if (seen.has(parsed.name)) {
                fail(
                    `\`${label.text}\`: there is already a track named ${parsed.name}`,
                );
            } else if (declared.has(parsed.name)) {
                fail(
                    `\`${label.text}\`: ${parsed.name} is already declared, and a track label declares its name too`,
                );
            } else {
                seen.add(parsed.name);
                tracks.push({
                    colonEnd: node.statement.pos,
                    expression: node.statement.expression,
                    labelEnd: end,
                    labelStart: start,
                    statement: node,
                    track: parsed,
                });
            }
        }
        ts.forEachChild(node, visit);
    };
    visit(file);
    const byName = new Map(tracks.map((t) => [t.track.name, t]));
    for (const statement of tracks) {
        const { group, groupStart } = statement.track;
        if (group === null) {
            continue;
        }
        const label = statement.statement.label.text;
        const start = statement.labelStart + groupStart;
        const fail = (message: string) =>
            errors.push({ end: statement.labelEnd, message, start });
        if (!byName.has(group)) {
            fail(`\`${label}\`: there is no track named ${group} to play into`);
            continue;
        }
        // A group that plays, through its own groups, back into itself.
        const visited = new Set([statement.track.name]);
        for (
            let next: string | null = group;
            next !== null;
            next = byName.get(next)?.track.group ?? null
        ) {
            if (visited.has(next)) {
                fail(
                    `\`${label}\`: ${statement.track.name} plays into ${group}, which plays back into ${next}`,
                );
                break;
            }
            visited.add(next);
        }
    }
    return {
        errors: errors.sort((a, b) => a.start - b.start),
        tracks,
    };
}
