/**
 * Every track sends to every bus, so a track whose signal is built from a
 * bus's audio would feed that bus back into itself. This finds such a track
 * in a built patch, however the bus reached it: a `$bus()` held in a const,
 * passed through a helper or an array, or driving the track's gain or pan.
 */
import { isCableRef } from '../patchSimilarityRemap';

interface BuiltModule {
    id: string;
    params: unknown;
}

export interface FeedbackTrack {
    label: string;
    /** The module whose output is the track's signal after pan and gain. */
    tapId: string;
    isBus: boolean;
    sourceLocation?: { line: number; column: number };
}

export interface FeedbackBus {
    name: string;
    /** The module whose output is the mix of the sends to the bus. */
    mixId: string;
}

/** Every module `params` takes a cable from, at any depth. */
function cableSources(params: unknown, into: string[]): void {
    if (Array.isArray(params)) {
        for (const item of params) {
            cableSources(item, into);
        }
        return;
    }
    if (typeof params !== 'object' || params === null) {
        return;
    }
    if (isCableRef(params)) {
        into.push(params.module);
        return;
    }
    for (const item of Object.values(params)) {
        cableSources(item, into);
    }
}

/**
 * The first track (in `tracks` order) that reads a bus, and the bus it
 * reads; null when none does. A track's walk stops at every other track's
 * tap, so a group reads a bus only through its own chain, not its members',
 * which are reported as themselves. A bus's own track reads its mix by
 * design and is never reported.
 */
export function findBusFeedback(
    modules: readonly BuiltModule[],
    tracks: readonly FeedbackTrack[],
    buses: readonly FeedbackBus[],
): { track: FeedbackTrack; bus: string } | null {
    if (buses.length === 0) {
        return null;
    }
    const sources = new Map<string, string[]>();
    for (const m of modules) {
        const into: string[] = [];
        cableSources(m.params, into);
        sources.set(m.id, into);
    }
    const busByMix = new Map(buses.map((b) => [b.mixId, b.name]));
    const taps = new Set(tracks.map((t) => t.tapId));
    for (const track of tracks) {
        if (track.isBus) {
            continue;
        }
        const seen = new Set([track.tapId]);
        const stack = [...(sources.get(track.tapId) ?? [])];
        while (stack.length > 0) {
            const id = stack.pop()!;
            if (seen.has(id) || taps.has(id)) {
                continue;
            }
            seen.add(id);
            const bus = busByMix.get(id);
            if (bus !== undefined) {
                return { bus, track };
            }
            stack.push(...(sources.get(id) ?? []));
        }
    }
    return null;
}
