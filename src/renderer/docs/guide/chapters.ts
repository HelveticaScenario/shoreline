import type { MDXContent } from 'mdx/types';
import { GUIDE_CHAPTERS } from './chapterList';

const pages = import.meta.glob<MDXContent>('./*.mdx', {
    eager: true,
    import: 'default',
});

/** Each chapter with its page. */
export const GUIDE_PAGES = GUIDE_CHAPTERS.map((chapter) => ({
    ...chapter,
    Page: pages[`./${chapter.id}.mdx`],
}));
