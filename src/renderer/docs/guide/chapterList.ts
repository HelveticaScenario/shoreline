/**
 * The guide's chapters in reading order. Each `id` names its page,
 * `<id>.mdx` beside this file; the first is the welcome page, which has no
 * chapter number.
 */
export interface GuideChapter {
    id: string;
    title: string;
    /** One line: what the chapter covers. */
    summary: string;
    /** The page's own heading and line under it, when they differ from the
     *  title and summary the chapter list shows. */
    heading?: string;
    lede?: string;
}

export const GUIDE_CHAPTERS: readonly GuideChapter[] = [
    {
        heading: 'Shoreline',
        id: 'welcome',
        lede: 'A modular synthesizer you play by writing code. Each line builds modules and patches them together; change a line, update, and the music carries on.',
        summary: 'A first sound, and where to go next.',
        title: 'Welcome',
    },
    {
        id: 'start',
        summary: 'Tracks, updating, and where everything lives.',
        title: 'Start here',
    },
    {
        id: 'sound',
        summary: 'Pitches, chaining modules, and modulating any parameter.',
        title: 'Pitch & sound',
    },
    {
        id: 'patterns',
        summary: 'Melodies, rhythms and drums in mini-notation.',
        title: 'Patterns',
    },
    {
        id: 'controls',
        summary: 'Sliders to play with and scopes to see your signals.',
        title: 'Controls & displays',
    },
    {
        id: 'mixing',
        summary: 'Track levels and panning, buses, sends and groups.',
        title: 'Mixing & routing',
    },
    {
        id: 'cheatsheet',
        summary: 'Everything on one page.',
        title: 'Cheat sheet',
    },
];

/** The anchor id of a section heading. */
export const sectionId = (heading: string) =>
    heading
        .toLowerCase()
        .replace(/[^a-z0-9]+/g, '-')
        .replace(/^-|-$/g, '');

/** The `## ` section headings of a chapter's source, in order. */
export const chapterSections = (source: string) =>
    [...source.matchAll(/^## (.+)$/gm)].map((m) => m[1].trim());
