/**
 * The guide's chapters in reading order. Each `id` names its page,
 * `<id>.mdx` beside this file; the first is the welcome page, which has no
 * chapter number.
 */
export interface GuideChapter {
    id: string;
    title: string;
    /** The line under the page's heading. */
    lede: string;
    /** The page's heading, when it differs from the title in the nav. */
    heading?: string;
}

export const GUIDE_CHAPTERS: readonly GuideChapter[] = [
    {
        heading: 'Shoreline',
        id: 'welcome',
        lede: 'A modular synthesizer you play by writing code. Every line builds modules and patches them together. Change a line, update, and the music carries on without missing a beat.',
        title: 'Welcome',
    },
    {
        id: 'start',
        lede: 'Tracks, updating, and where everything lives.',
        title: 'Start here',
    },
    {
        id: 'sound',
        lede: 'Pitches, chaining modules, and modulating any parameter.',
        title: 'Pitch & sound',
    },
    {
        id: 'patterns',
        lede: 'Melodies, rhythms and drums in mini-notation.',
        title: 'Patterns',
    },
    {
        id: 'controls',
        lede: 'Sliders and buttons to play, and scopes to see your signals.',
        title: 'Controls & displays',
    },
    {
        id: 'mixing',
        lede: 'Levels and panning, shared effects and groups.',
        title: 'Mixing & routing',
    },
    {
        id: 'cheatsheet',
        lede: 'Everything on one page.',
        title: 'Cheat sheet',
    },
];

/** The anchor id of a section heading. */
export const sectionId = (heading: string) =>
    heading
        .toLowerCase()
        .replace(/[^a-z0-9]+/g, '-')
        .replace(/^-|-$/g, '');
