/**
 * Which folds a run shows to be no fold: a value that may be stereo only
 * because of the files some module calls play is mono when every run of
 * each of those calls built a mono module.
 */

import type {
    Channels,
    ModuleChannelsRecord,
} from '../../../shared/dsl/channels';
import {
    type FileCall,
    recordColumn,
    type SignalSite,
} from '../../../shared/dsl/signalSites';

/** A file call by its document offsets and the module it builds. */
export function callKey(start: number, end: number, module: string): string {
    return `${start}:${end}:${module}`;
}

/**
 * The file calls among `sites`' folds that every run recorded in `records`
 * found mono. A call that never ran is not among them.
 */
export function monoFileCalls(
    sites: readonly SignalSite[],
    records: readonly ModuleChannelsRecord[],
): FileCall[] {
    const runs = new Map<string, Channels[]>();
    for (const record of records) {
        const key = `${record.line}:${recordColumn(record)}:${record.module}`;
        runs.set(key, [...(runs.get(key) ?? []), record.channels]);
    }
    const mono = new Map<string, FileCall>();
    for (const call of sites.flatMap((site) => site.foldCalls ?? [])) {
        const channels = runs.get(`${call.line}:${call.column}:${call.module}`);
        if (channels?.every((c) => c === 'mono')) {
            mono.set(callKey(call.start, call.end, call.module), call);
        }
    }
    return [...mono.values()];
}

/**
 * Whether `site` folds, given the keys of the file calls a run found mono
 * whose text is unchanged.
 */
export function foldsGiven(
    site: SignalSite,
    mono: ReadonlySet<string>,
): boolean {
    return (
        site.fold &&
        !site.foldCalls?.every((call) =>
            mono.has(callKey(call.start, call.end, call.module)),
        )
    );
}
