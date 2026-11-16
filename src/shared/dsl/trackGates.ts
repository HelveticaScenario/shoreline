/**
 * Which tracks are audible, from their mute/solo states and the groups they
 * play into. With no solo anywhere a track plays unless muted. Once any track
 * is soloed, a track plays if it is soloed itself, or is unmuted and either
 * plays into a soloed group or carries a soloed track, so a soloed member is
 * heard through its group and a soloed group is heard with all its members.
 * The main engine and the mixer's live toggles both gate by this.
 */
export interface GatedTrack {
    key: string;
    mute: boolean;
    solo: boolean;
    /** The group track this track plays into, by key. */
    group?: string | null;
}

/** Each track's key with whether its gate is open. */
export function trackGates(
    tracks: readonly GatedTrack[],
): Map<string, boolean> {
    const byKey = new Map(tracks.map((t) => [t.key, t]));
    const gates = new Map<string, boolean>();
    if (!tracks.some((t) => t.solo)) {
        for (const t of tracks) {
            gates.set(t.key, !t.mute);
        }
        return gates;
    }
    // Every group with a soloed track somewhere inside it.
    const carriesSolo = new Set<string>();
    for (const t of tracks) {
        if (!t.solo) {
            continue;
        }
        const seen = new Set<string>();
        for (
            let g = t.group ?? null;
            g !== null && !seen.has(g);
            g = byKey.get(g)?.group ?? null
        ) {
            seen.add(g);
            carriesSolo.add(g);
        }
    }
    const inSoloedGroup = (t: GatedTrack): boolean => {
        const seen = new Set<string>();
        for (
            let g = t.group ?? null;
            g !== null && !seen.has(g);
            g = byKey.get(g)?.group ?? null
        ) {
            seen.add(g);
            if (byKey.get(g)?.solo) {
                return true;
            }
        }
        return false;
    };
    for (const t of tracks) {
        gates.set(
            t.key,
            t.solo || (!t.mute && (carriesSolo.has(t.key) || inSoloedGroup(t))),
        );
    }
    return gates;
}
