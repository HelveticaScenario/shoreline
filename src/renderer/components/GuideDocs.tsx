import React, { useLayoutEffect, useRef } from 'react';
import type { MDXComponents } from 'mdx/types';
import { GUIDE_PAGES } from '../docs/guide/chapters';
import { sectionId } from '../docs/guide/chapterList';
import { CodeExample } from './CodeExample';
import { ColorizedCode } from './ColorizedCode';
import { KeyChips } from './KeyChips';
import { useQuoteExample } from '../hooks/useQuoteExample';
import { useCommandBinding } from '../keybindings/useCommandBinding';

/** The guide's `<Key command="…" />`: a command's current shortcut. */
const Key: React.FC<{ command: string }> = ({ command }) => {
    const binding = useCommandBinding(command);
    return binding ? <KeyChips binding={binding} /> : <em>no shortcut</em>;
};

/** The guide's `<Tip title="…">`: an aside set off from the text. */
const Tip: React.FC<{ title: string; children?: React.ReactNode }> = ({
    title,
    children,
}) => (
    <aside className="guide-tip">
        <div className="guide-eyebrow">{title}</div>
        {children}
    </aside>
);

/** A chapter's label in the guide: its number, or nothing for the welcome page. */
const chapterNumber = (index: number) => (index === 0 ? '' : String(index));

const textOf = (children: React.ReactNode): string =>
    React.Children.toArray(children)
        .map((child) => (typeof child === 'string' ? child : ''))
        .join('');

/** Inline code in the guide, in the editor's colours and quote style. */
const InlineCode: React.FC<{ children?: React.ReactNode }> = ({ children }) => {
    const quote = useQuoteExample();
    return typeof children === 'string' ? (
        <ColorizedCode code={quote(children)} inline />
    ) : (
        <code>{children}</code>
    );
};

/** A block example in the guide; MDX hands it over as `pre > code`. */
const BlockCode: React.FC<{ children?: React.ReactNode }> = ({ children }) => {
    const quote = useQuoteExample();
    const code = React.isValidElement<{ children?: unknown }>(children)
        ? children.props.children
        : null;
    if (typeof code !== 'string') {
        return <pre>{children}</pre>;
    }
    return <CodeExample code={quote(code.replace(/\n$/, ''))} />;
};

// Module-level, so the page keeps its elements, and the examples their
// editors, across renders. Headings sit one level down, so the page title is
// the `h2` the other pages use.
const COMPONENTS: MDXComponents = {
    Key,
    Tip,
    // Block code renders through `pre`, so `code` here is always inline.
    code: InlineCode,
    h2: ({ children }) => <h3 id={sectionId(textOf(children))}>{children}</h3>,
    pre: BlockCode,
    table: ({ children }) => <table className="doc-table">{children}</table>,
};

interface GuideDocsProps {
    chapterId: string;
    onChapterChange: (id: string) => void;
}

/** The Docs panel's Guide page: chapters in reading order, one at a time. */
export const GuideDocs: React.FC<GuideDocsProps> = ({
    chapterId,
    onChapterChange,
}) => {
    const pageRef = useRef<HTMLDivElement>(null);
    const index = Math.max(
        0,
        GUIDE_PAGES.findIndex((c) => c.id === chapterId),
    );
    const chapter = GUIDE_PAGES[index];
    const prev = GUIDE_PAGES[index - 1];
    const next = GUIDE_PAGES[index + 1];

    useLayoutEffect(() => {
        pageRef.current?.scrollTo({ top: 0 });
    }, [chapterId]);

    // Scrolls the page alone; the app containers around it must not move.
    const scrollToSection = (heading: string) => {
        const page = pageRef.current;
        const target = page?.querySelector(`#${sectionId(heading)}`);
        if (!page || !target) {
            return;
        }
        const offset =
            target.getBoundingClientRect().top -
            page.getBoundingClientRect().top;
        page.scrollTo({ behavior: 'smooth', top: page.scrollTop + offset });
    };

    const { Page } = chapter;
    return (
        <div className="guide">
            <nav className="guide-nav" aria-label="Guide chapters">
                <ol>
                    {GUIDE_PAGES.map((c, i) => (
                        <li key={c.id}>
                            <button
                                aria-current={
                                    c.id === chapter.id ? 'page' : undefined
                                }
                                className={c.id === chapter.id ? 'active' : ''}
                                onClick={() => onChapterChange(c.id)}
                            >
                                <span className="guide-number">
                                    {chapterNumber(i)}
                                </span>
                                {c.title}
                            </button>
                            {c.id === chapter.id && c.sections.length > 0 && (
                                <ul>
                                    {c.sections.map((heading) => (
                                        <li key={heading}>
                                            <button
                                                onClick={() =>
                                                    scrollToSection(heading)
                                                }
                                            >
                                                {heading}
                                            </button>
                                        </li>
                                    ))}
                                </ul>
                            )}
                        </li>
                    ))}
                </ol>
            </nav>
            <div ref={pageRef} className="guide-page">
                <header className="guide-header">
                    <div className="guide-eyebrow">
                        {index === 0 ? 'The guide' : `Chapter ${index}`}
                    </div>
                    <h2>{chapter.heading ?? chapter.title}</h2>
                    <p className="guide-lede">
                        {chapter.lede ?? chapter.summary}
                    </p>
                </header>
                <Page components={COMPONENTS} />
                <footer className="guide-pager">
                    {prev && (
                        <button onClick={() => onChapterChange(prev.id)}>
                            <span className="guide-eyebrow">← previous</span>
                            {prev.title}
                        </button>
                    )}
                    {next && (
                        <button
                            className="guide-pager-next"
                            onClick={() => onChapterChange(next.id)}
                        >
                            <span className="guide-eyebrow">next →</span>
                            {next.title}
                        </button>
                    )}
                </footer>
            </div>
        </div>
    );
};
