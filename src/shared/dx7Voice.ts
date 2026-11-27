import type { SyxVoiceInfo } from '@modular/core';

/** One voice of a `.syx` file as the engine reports it. */
export type SyxVoice = SyxVoiceInfo;

/** The voice names of one workspace `.syx` file. */
export interface SyxFileVoices {
    path: string;
    names: string[];
}

/** A fragment naming a voice by its index in the file. */
const INDEX_FRAGMENT = /^@\d+$/;

/** The `voice` param value the engine reads: the voice's bytes themselves. */
export interface Dx7VoiceRef {
    type: 'dx7_voice';
    path: string;
    name: string;
    data: string;
}

/** Whether `value` is already the engine's voice reference. */
export function isDx7VoiceRef(value: unknown): value is Dx7VoiceRef {
    return (
        typeof value === 'object' &&
        value !== null &&
        'type' in value &&
        value.type === 'dx7_voice'
    );
}

/**
 * `value` split into its file and voice: `/dx7/rom1a.syx#E.PIANO 1` names a
 * voice in a file, `/dx7/rom1a.syx` the file's first voice. The voice part
 * starts at the first `#` after `.syx`, so a name may hold `#`.
 */
export function splitVoice(value: string): {
    path: string;
    voice: string | null;
} {
    const match = /\.syx#/i.exec(value);
    if (match === null) return { path: value, voice: null };
    return {
        path: value.slice(0, match.index + 4),
        voice: value.slice(match.index + 5),
    };
}

/**
 * The fragment that names each voice of a file: its name, or `@index` for a
 * voice whose name an earlier voice already has or reads as an index.
 */
export function voiceFragments(names: readonly string[]): string[] {
    const seen = new Set<string>();
    return names.map((name, index) => {
        if (seen.has(name) || INDEX_FRAGMENT.test(name)) return `@${index}`;
        seen.add(name);
        return name;
    });
}

const LISTED_NAMES = 40;

/**
 * The voice `fragment` names among `voices` (the first when null), or an error
 * naming `fn` that lists the file's voices.
 */
export function selectVoice(
    fn: string,
    path: string,
    fragment: string | null,
    voices: readonly SyxVoice[],
): Dx7VoiceRef {
    const fragments = voiceFragments(voices.map((v) => v.name));
    const index =
        fragment === null
            ? 0
            : INDEX_FRAGMENT.test(fragment)
              ? Number(fragment.slice(1))
              : fragments.indexOf(fragment);
    const voice = voices[index];
    if (voice === undefined) {
        const listed = fragments
            .slice(0, LISTED_NAMES)
            .map((f) => JSON.stringify(f))
            .join(', ');
        const more =
            fragments.length > LISTED_NAMES
                ? ` and ${fragments.length - LISTED_NAMES} more`
                : '';
        throw new Error(
            `${fn}: ${path} has no voice ${JSON.stringify(fragment)}; its voices are ${listed}${more}`,
        );
    }
    return { type: 'dx7_voice', path, name: voice.name, data: voice.data };
}
