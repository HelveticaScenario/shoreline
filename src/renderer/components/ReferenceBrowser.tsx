import React, { memo, useCallback, useLayoutEffect, useRef } from 'react';
import { measureElement, useVirtualizer } from '@tanstack/react-virtual';
import referenceCategories from '@modular/core/referenceCategories.json';
import {
    Group,
    Panel,
    type PanelImperativeHandle,
    Separator,
    usePanelRef,
} from 'react-resizable-panels';
import { useStoredPanelSize } from '../hooks/useStoredPanelSize';
import { ReferenceCard } from './ReferenceCard';
import type { DslTypeName } from '../../shared/dsl/typeDocs';
import type { ReferenceEntry } from '../../shared/dsl/referenceDocs';
import { allExamples } from '../../shared/dsl/referenceDocs';

const CARD_GAP_PX = 15;
const ESTIMATED_CARD_HEIGHT_PX = 320;

const INDEX_ID = 'docs-reference-index';
const WIDTH_STORAGE_KEY = 'modular_docs_index_width';
const INDEX_DEFAULT_WIDTH = 200;
const INDEX_MIN_WIDTH = 120;
/** Room kept for the cards when the index grows. */
const CARDS_MIN_WIDTH = 240;

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
    const indexRef = usePanelRef();
    const indexWidth = useStoredPanelSize(
        WIDTH_STORAGE_KEY,
        INDEX_DEFAULT_WIDTH,
        indexRef,
    );

    const used = new Set(entries.flatMap((e) => e.docs.categories));
    const categories = referenceCategories.filter((c) => used.has(c.name));

    const query = searchQuery.toLowerCase();
    // Each rank keeps the display order.
    const filteredEntries = entries
        .filter((e) => !category || e.docs.categories.includes(category))
        .map((entry) => ({ entry, rank: searchRank(entry, query) }))
        .filter(({ rank }) => rank >= 0)
        .sort((a, b) => a.rank - b.rank)
        .map(({ entry }) => entry);

    const { scrollRef, list, topName, scrollToEntry } = useVirtualCards(
        filteredEntries,
        searchQuery,
        category,
        onTypeClick,
    );

    return (
        <Group
            className="reference-browser"
            orientation="horizontal"
            onLayoutChanged={indexWidth.onLayoutChanged}
        >
            <ReferenceIndex
                noun={noun}
                entries={filteredEntries}
                topName={topName}
                onSelect={scrollToEntry}
                searchQuery={searchQuery}
                onSearchQueryChange={onSearchQueryChange}
                category={category}
                panelRef={indexRef}
                defaultWidth={indexWidth.defaultSize}
            />
            <Separator
                className="app-sash into-previous reference-index-sash"
                aria-label={`Resize ${noun} index`}
            />
            <Panel
                style={{ overflow: 'hidden' }}
                id="docs-reference-cards"
                className="reference-browser-main"
                minSize={CARDS_MIN_WIDTH}
            >
                {categories.length > 0 && (
                    <ReferenceCategories
                        categories={categories}
                        category={category}
                        onCategoryChange={onCategoryChange}
                    />
                )}
                <div ref={scrollRef} className="reference-cards">
                    {filteredEntries.length === 0 && entries.length > 0 && (
                        <p className="reference-browser-empty">
                            No {noun} match
                            {category ? ` in ${category}` : ''}.
                        </p>
                    )}
                    {list}
                </div>
            </Panel>
        </Group>
    );
};

interface ReferenceIndexProps {
    noun: string;
    entries: ReferenceEntry[];
    /** The entry whose card is at the top of the cards pane. */
    topName: string | undefined;
    onSelect: (index: number) => void;
    searchQuery: string;
    onSearchQueryChange: (query: string) => void;
    category: string | null;
    panelRef: React.RefObject<PanelImperativeHandle | null>;
    defaultWidth: number;
}

/**
 * The search field and the list of entry names, which follows the cards'
 * scroll. `memo` because the browser around it renders on every scroll
 * frame.
 */
const ReferenceIndex = memo(function ReferenceIndex({
    noun,
    entries,
    topName,
    onSelect,
    searchQuery,
    onSearchQueryChange,
    category,
    panelRef,
    defaultWidth,
}: ReferenceIndexProps) {
    const listRef = useRef<HTMLUListElement>(null);

    // A new filter shows its names from their left edge.
    useLayoutEffect(() => {
        if (listRef.current) {
            listRef.current.scrollLeft = 0;
        }
    }, [searchQuery, category]);

    // Scrolls the index alone; the panes around it must not move.
    useLayoutEffect(() => {
        const list = listRef.current;
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
    }, [topName, entries]);

    return (
        <Panel
            style={{ overflow: 'hidden' }}
            id={INDEX_ID}
            className="reference-index"
            panelRef={panelRef}
            defaultSize={defaultWidth}
            minSize={INDEX_MIN_WIDTH}
            groupResizeBehavior="preserve-pixel-size"
        >
            <input
                type="text"
                placeholder={`Search ${noun}...`}
                aria-label={`Search ${noun}`}
                value={searchQuery}
                onChange={(e) => onSearchQueryChange(e.target.value)}
                className="search-input"
            />
            <ul
                ref={listRef}
                className="reference-index-list"
                aria-label={noun}
            >
                {entries.map((entry, index) => (
                    <li key={entry.name}>
                        <button
                            className={entry.name === topName ? 'active' : ''}
                            aria-current={entry.name === topName}
                            title={entry.docs.summary}
                            onClick={() => onSelect(index)}
                        >
                            {entry.name}
                        </button>
                    </li>
                ))}
            </ul>
        </Panel>
    );
});

interface ReferenceCategoriesProps {
    categories: typeof referenceCategories;
    category: string | null;
    onCategoryChange: (category: string | null) => void;
}

/** One toggle per category; pressing the active one lists every category again. */
const ReferenceCategories: React.FC<ReferenceCategoriesProps> = ({
    categories,
    category,
    onCategoryChange,
}) => (
    <div className="reference-categories" role="group" aria-label="Categories">
        {categories.map(({ name, description }) => (
            <button
                key={name}
                aria-pressed={category === name}
                className={category === name ? 'active' : ''}
                title={description}
                onClick={() =>
                    onCategoryChange(category === name ? null : name)
                }
            >
                {name}
            </button>
        ))}
    </div>
);

interface VirtualCards {
    /** Goes on the element that scrolls the cards. */
    scrollRef: React.RefObject<HTMLDivElement | null>;
    /** The cards near the viewport, in a box as tall as all of them. */
    list: React.ReactElement;
    /** The entry whose card is at the top of the scroll element. */
    topName: string | undefined;
    /** Scrolls the card of the entry at `index` to the top. */
    scrollToEntry: (index: number) => void;
}

/**
 * Virtualizes the cards of `entries`, scrolling back to the first whenever
 * `searchQuery` or `category` changes. `useVirtualizer` returns a mutable
 * instance whose methods read fresh state on every render, so React Compiler
 * must not memoize anything derived from it: everything that reads the
 * instance, the cards' markup included, is inside this hook.
 */
function useVirtualCards(
    entries: ReferenceEntry[],
    searchQuery: string,
    category: string | null,
    onTypeClick: (typeName: DslTypeName) => void,
): VirtualCards {
    'use no memo';
    const scrollRef = useRef<HTMLDivElement>(null);

    const getItemKey = useCallback(
        (index: number) => entries[index].name,
        [entries],
    );

    // oxlint-disable-next-line react-hooks-js/incompatible-library
    const virtualizer = useVirtualizer({
        count: entries.length,
        estimateSize: () => ESTIMATED_CARD_HEIGHT_PX,
        gap: CARD_GAP_PX,
        getItemKey,
        getScrollElement: () => scrollRef.current,
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
    }, [virtualizer, searchQuery, category]);

    // Stable while the cards scroll, so the index re-renders only when its top entry changes.
    const scrollToEntry = useCallback(
        (index: number) => virtualizer.scrollToIndex(index, { align: 'start' }),
        [virtualizer],
    );

    const virtualItems = virtualizer.getVirtualItems();
    const scrollOffset = virtualizer.scrollOffset ?? 0;
    // The gap belongs to the card above it, and a scroll that lands a
    // sub-pixel short of a card's start still counts as reaching it.
    const topItem = virtualItems.find(
        (item) => item.end + CARD_GAP_PX - 1 > scrollOffset,
    );
    const topName = topItem && entries[topItem.index]?.name;

    const list = (
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
                    style={{ transform: `translateY(${item.start}px)` }}
                >
                    <ReferenceCard
                        entry={entries[item.index]}
                        onTypeClick={onTypeClick}
                    />
                </div>
            ))}
        </div>
    );

    return { scrollRef, list, topName, scrollToEntry };
}
