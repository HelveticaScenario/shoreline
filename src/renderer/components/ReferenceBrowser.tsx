import React, { useCallback, useLayoutEffect, useMemo, useRef } from 'react';
import { measureElement, useVirtualizer } from '@tanstack/react-virtual';
import referenceCategories from '@modular/core/referenceCategories.json';
import { ReferenceCard } from './ReferenceCard';
import { Sash } from './Sash';
import { useStoredSize } from '../hooks/useStoredSize';
import type { DslTypeName } from '../../shared/dsl/typeDocs';
import type { ReferenceEntry } from '../../shared/dsl/referenceDocs';
import { allExamples } from '../../shared/dsl/referenceDocs';

const CARD_GAP_PX = 15;
const ESTIMATED_CARD_HEIGHT_PX = 320;

const INDEX_ID = 'docs-reference-index';
const INDEX_WIDTH_STORAGE_KEY = 'modular_docs_index_width';
const INDEX_MIN_WIDTH = 120;
/** Room kept for the cards when the index grows. */
const CARDS_MIN_WIDTH = 240;

function indexMaxWidth(): number {
    const reference = document.getElementById(INDEX_ID)?.parentElement;
    const width = reference?.getBoundingClientRect().width ?? 0;
    return Math.max(INDEX_MIN_WIDTH, width - CARDS_MIN_WIDTH);
}

const clampIndexWidth = (width: number) => Math.max(width, INDEX_MIN_WIDTH);

interface ReferenceBrowserProps {
    /** Already sorted in display order. */
    entries: ReferenceEntry[];
    /** What the entries are, in the plural: `modules`, `globals`. */
    noun: string;
    searchQuery: string;
    onSearchQueryChange: (query: string) => void;
    /** Only entries in this category are listed; `null` lists every category. */
    category: string | null;
    onCategoryChange: (category: string | null) => void;
    onTypeClick: (typeName: DslTypeName) => void;
}

/** Name matches lead, best first; then entries whose summary or description
 *  mention the query; then those whose params or examples do. -1 is no match. */
function searchRank(entry: ReferenceEntry, query: string): number {
    const name = entry.name.toLowerCase();
    if (name === query) return 0;
    if (name.startsWith(query)) return 1;
    if (name.includes(query)) return 2;
    const { summary, description, sections } = entry.docs;
    if (`${summary}\n${description}`.toLowerCase().includes(query)) {
        return 3;
    }
    const rest = [
        ...entry.params.flatMap((p) => [p.name, p.description ?? '']),
        ...sections.flatMap((s) => [s.title, s.body]),
        ...allExamples(entry.docs).flatMap((e) => [e.title, e.code]),
    ];
    return rest.join('\n').toLowerCase().includes(query) ? 4 : -1;
}

/**
 * A searchable name index beside the reference cards. The index and the
 * cards show the same filtered entries; the cards render through a
 * virtualizer so only those near the viewport are mounted.
 */
export const ReferenceBrowser: React.FC<ReferenceBrowserProps> = ({
    entries,
    noun,
    searchQuery,
    onSearchQueryChange,
    category,
    onCategoryChange,
    onTypeClick,
}) => {
    const [indexWidth, setIndexWidth] = useStoredSize(
        INDEX_WIDTH_STORAGE_KEY,
        clampIndexWidth,
    );

    const categories = useMemo(() => {
        const used = new Set(entries.flatMap((e) => e.docs.categories));
        return referenceCategories.filter((c) => used.has(c.name));
    }, [entries]);

    const filteredEntries = useMemo(() => {
        const query = searchQuery.toLowerCase();
        // Each rank keeps the display order.
        return entries
            .filter((e) => !category || e.docs.categories.includes(category))
            .map((entry) => ({ entry, rank: searchRank(entry, query) }))
            .filter(({ rank }) => rank >= 0)
            .sort((a, b) => a.rank - b.rank)
            .map(({ entry }) => entry);
    }, [entries, searchQuery, category]);

    return (
        <ReferenceList
            noun={noun}
            hasEntries={entries.length > 0}
            filteredEntries={filteredEntries}
            categories={categories}
            searchQuery={searchQuery}
            onSearchQueryChange={onSearchQueryChange}
            category={category}
            onCategoryChange={onCategoryChange}
            indexWidth={indexWidth}
            onIndexWidthChange={setIndexWidth}
            onTypeClick={onTypeClick}
        />
    );
};

interface ReferenceListProps {
    noun: string;
    hasEntries: boolean;
    filteredEntries: ReferenceEntry[];
    categories: typeof referenceCategories;
    searchQuery: string;
    onSearchQueryChange: (query: string) => void;
    category: string | null;
    onCategoryChange: (category: string | null) => void;
    indexWidth: number | null;
    onIndexWidthChange: (width: number | null) => void;
    onTypeClick: (typeName: DslTypeName) => void;
}

/**
 * The name index and the virtualized cards. `useVirtualizer` returns a mutable
 * instance whose methods read fresh state on every render, so React Compiler
 * must not memoize anything derived from it.
 */
const ReferenceList: React.FC<ReferenceListProps> = ({
    noun,
    hasEntries,
    filteredEntries,
    categories,
    searchQuery,
    onSearchQueryChange,
    category,
    onCategoryChange,
    indexWidth,
    onIndexWidthChange,
    onTypeClick,
}) => {
    'use no memo';
    const cardsRef = useRef<HTMLDivElement>(null);
    const indexRef = useRef<HTMLUListElement>(null);

    const getItemKey = useCallback(
        (index: number) => filteredEntries[index].name,
        [filteredEntries],
    );

    // oxlint-disable-next-line react-hooks-js/incompatible-library
    const virtualizer = useVirtualizer({
        count: filteredEntries.length,
        estimateSize: () => ESTIMATED_CARD_HEIGHT_PX,
        gap: CARD_GAP_PX,
        getItemKey,
        getScrollElement: () => cardsRef.current,
        // A hidden pane measures every card as zero; keep the sizes they had while shown.
        measureElement: (element, entry, instance) => {
            const size = measureElement(element, entry, instance);
            if (size > 0) {
                return size;
            }
            const key = instance.options.getItemKey(
                instance.indexFromElement(element),
            );
            return instance.itemSizeCache.get(key) ?? size;
        },
        overscan: 3,
        useFlushSync: false,
    });

    // A new filter starts its results from the first match. Scrolling through the
    // virtualizer keeps its offset in step, so re-measuring cannot shift the pane back.
    useLayoutEffect(() => {
        virtualizer.scrollToOffset(0);
        if (indexRef.current) {
            indexRef.current.scrollLeft = 0;
        }
    }, [virtualizer, searchQuery, category]);

    const virtualItems = virtualizer.getVirtualItems();
    const scrollOffset = virtualizer.scrollOffset ?? 0;
    // The gap belongs to the card above it, and a scroll that lands a
    // sub-pixel short of a card's start still counts as reaching it.
    const topItem = virtualItems.find(
        (item) => item.end + CARD_GAP_PX - 1 > scrollOffset,
    );
    const topName = topItem && filteredEntries[topItem.index]?.name;

    // Scrolls the index alone; the panes around it must not move.
    useLayoutEffect(() => {
        const list = indexRef.current;
        const active = list?.querySelector<HTMLElement>('button.active');
        if (!list || !active) {
            return;
        }
        const top =
            active.getBoundingClientRect().top -
            list.getBoundingClientRect().top +
            list.scrollTop;
        const bottom = top + active.offsetHeight;
        if (top < list.scrollTop) {
            list.scrollTop = top;
        } else if (bottom > list.scrollTop + list.clientHeight) {
            list.scrollTop = bottom - list.clientHeight;
        }
    }, [topName, filteredEntries]);

    return (
        <div className="reference-browser">
            <div
                id={INDEX_ID}
                className="reference-index"
                style={{
                    width: indexWidth ?? undefined,
                    maxWidth: `calc(100% - ${CARDS_MIN_WIDTH}px)`,
                }}
            >
                <Sash
                    panelId={INDEX_ID}
                    axis="x"
                    grow={1}
                    min={INDEX_MIN_WIDTH}
                    max={indexMaxWidth}
                    label={`Resize ${noun} index`}
                    className="reference-index-sash"
                    onSizeChange={onIndexWidthChange}
                />
                <input
                    type="text"
                    placeholder={`Search ${noun}...`}
                    aria-label={`Search ${noun}`}
                    value={searchQuery}
                    onChange={(e) => onSearchQueryChange(e.target.value)}
                    className="search-input"
                />
                <ul
                    ref={indexRef}
                    className="reference-index-list"
                    aria-label={noun}
                >
                    {filteredEntries.map((entry, index) => (
                        <li key={entry.name}>
                            <button
                                className={
                                    entry.name === topName ? 'active' : ''
                                }
                                aria-current={entry.name === topName}
                                title={entry.docs.summary}
                                onClick={() =>
                                    virtualizer.scrollToIndex(index, {
                                        align: 'start',
                                    })
                                }
                            >
                                {entry.name}
                            </button>
                        </li>
                    ))}
                </ul>
            </div>
            <div className="reference-browser-main">
                {categories.length > 0 && (
                    <div
                        className="reference-categories"
                        role="group"
                        aria-label="Categories"
                    >
                        {categories.map(({ name, description }) => (
                            <button
                                key={name}
                                aria-pressed={category === name}
                                className={category === name ? 'active' : ''}
                                title={description}
                                onClick={() =>
                                    onCategoryChange(
                                        category === name ? null : name,
                                    )
                                }
                            >
                                {name}
                            </button>
                        ))}
                    </div>
                )}
                <div ref={cardsRef} className="reference-cards">
                    {filteredEntries.length === 0 && hasEntries && (
                        <p className="reference-browser-empty">
                            No {noun} match
                            {category ? ` in ${category}` : ''}.
                        </p>
                    )}
                    <div
                        className="reference-list"
                        style={{ height: virtualizer.getTotalSize() }}
                    >
                        {virtualItems.map((item) => (
                            <div
                                key={item.key}
                                ref={virtualizer.measureElement}
                                data-index={item.index}
                                className="reference-list-item"
                                style={{
                                    transform: `translateY(${item.start}px)`,
                                }}
                            >
                                <ReferenceCard
                                    entry={filteredEntries[item.index]}
                                    onTypeClick={onTypeClick}
                                />
                            </div>
                        ))}
                    </div>
                </div>
            </div>
        </div>
    );
};
