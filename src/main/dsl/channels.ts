/**
 * Whether a signal is mono or stereo, as far as the DSL can tell without
 * running the patch. The engine settles the truth when it builds the patch;
 * `'unknown'` stands for a signal only the engine can settle (a deferred
 * one, or a player whose file decides), and the editor treats it as stereo.
 */

export type Channels = 'mono' | 'stereo' | 'unknown';

/**
 * The channels of a module that lifts to stereo when any input it reads
 * lane by lane is stereo.
 */
export function liftedChannels(inputs: Iterable<Channels>): Channels {
    let channels: Channels = 'mono';
    for (const input of inputs) {
        if (input === 'stereo') {
            return 'stereo';
        }
        if (input === 'unknown') {
            channels = 'unknown';
        }
    }
    return channels;
}

/**
 * The channels of a collection of lanes: mono or stereo when every lane is,
 * otherwise unknown.
 */
export function collectionChannels(lanes: Iterable<Channels>): Channels {
    let channels: Channels | undefined;
    for (const lane of lanes) {
        if (channels === undefined) {
            channels = lane;
        } else if (channels !== lane) {
            return 'unknown';
        }
    }
    return channels ?? 'mono';
}
