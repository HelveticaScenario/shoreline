import type { MDXContent } from 'mdx/types';
import { GUIDE_CHAPTERS, chapterSections } from './chapterList';

const pages = import.meta.glob<MDXContent>('./*.mdx', {
    eager: true,
    import: 'default',
});
const sources = import.meta.glob<string>('./*.mdx', {
    eager: true,
    import: 'default',
    query: '?raw',
});

/** Each chapter with its page and the headings of its sections. */
export const GUIDE_PAGES = GUIDE_CHAPTERS.map((chapter) => ({
    ...chapter,
    Page: pages[`./${chapter.id}.mdx`],
    sections: chapterSections(sources[`./${chapter.id}.mdx`]),
}));
