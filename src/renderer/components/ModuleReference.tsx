import React, { useCallback, useLayoutEffect, useMemo, useRef } from 'react';
import { measureElement, useVirtualizer } from '@tanstack/react-virtual';
import moduleCategories from '@modular/core/moduleCategories.json';
import { ReferenceCard } from './ReferenceCard';
import { Sash } from './Sash';
import { useStoredSize } from '../hooks/useStoredSize';
import type { DslTypeName } from '../../shared/dsl/typeDocs';
import type { Schema } from '../../shared/dsl/schemaTypeResolver';
import type { ReferenceEntry } from '../../shared/dsl/referenceDocs';
import { moduleToReferenceEntry } from '../../shared/dsl/referenceDocs';

const CARD_GAP_PX = 15;
const ESTIMATED_CARD_HEIGHT_PX = 320;

const INDEX_ID = 'docs-module-index';
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

interface ModuleReferenceProps {
    /** Already sorted in display order. */
    modules: Schema[];
    searchQuery: string;
    onSearchQueryChange: (query: string) => void;
    /** Only modules in this category are listed; `null` lists every category. */
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
    const { summary, description, sections, examples } = entry.docs;
    if (`${summary}\n${description}`.toLowerCase().includes(query)) {
        return 3;
    }
    const rest = [
        ...entry.params.flatMap((p) => [p.name, p.description ?? '']),
        ...sections.flatMap((s) => [s.title, s.body]),
        ...[...examples, ...sections.flatMap((s) => s.examples)].flatMap(
            (e) => [e.title, e.code],
        ),
    ];
    return rest.join('\n').toLowerCase().includes(query) ? 4 : -1;
}

/**
 * A searchable name index beside the module cards. The index and the cards
 * show the same filtered entries; the cards render through a virtualizer so
 * only those near the viewport are mounted.
 */
export const ModuleReference: React.FC<ModuleReferenceProps> = ({
    modules,
    searchQuery,
    onSearchQueryChange,
    category,
    onCategoryChange,
    onTypeClick,
}) => {
    const cardsRef = useRef<HTMLDivElement>(null);
    const indexRef = useRef<HTMLUListElement>(null);
    const [indexWidth, setIndexWidth] = useStoredSize(
        INDEX_WIDTH_STORAGE_KEY,
        clampIndexWidth,
    );

    const entries = useMemo(
        () => modules.map(moduleToReferenceEntry),
        [modules],
    );

    const categories = useMemo(() => {
        const used = new Set(entries.flatMap((e) => e.docs.categories));
        return moduleCategories.filter((c) => used.has(c.name));
    }, [entries]);

    const filteredModules = useMemo(() => {
        const query = searchQuery.toLowerCase();
        // Each rank keeps the display order.
        return entries
            .filter((e) => !category || e.docs.categories.includes(category))
            .map((entry) => ({ entry, rank: searchRank(entry, query) }))
            .filter(({ rank }) => rank >= 0)
            .sort((a, b) => a.rank - b.rank)
            .map(({ entry }) => entry);
    }, [entries, searchQuery, category]);

    const getItemKey = useCallback(
        (index: number) => filteredModules[index].name,
        [filteredModules],
    );

    const virtualizer = useVirtualizer({
        count: filteredModules.length,
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
    const topName = topItem && filteredModules[topItem.index]?.name;

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
    }, [topName, filteredModules]);

    return (
        <div className="module-reference">
            <div
                id={INDEX_ID}
                className="module-index"
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
                    label="Resize module index"
                    className="module-index-sash"
                    onSizeChange={setIndexWidth}
                />
                <input
                    type="text"
                    placeholder="Search modules..."
                    aria-label="Search modules"
                    value={searchQuery}
                    onChange={(e) => onSearchQueryChange(e.target.value)}
                    className="search-input"
                />
                <ul
                    ref={indexRef}
                    className="module-index-list"
                    aria-label="Modules"
                >
                    {filteredModules.map((entry, index) => (
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
            <div className="module-reference-main">
                {categories.length > 0 && (
                    <div
                        className="module-categories"
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
                <div ref={cardsRef} className="module-cards">
                    {filteredModules.length === 0 && modules.length > 0 && (
                        <p className="module-reference-empty">
                            No modules match
                            {category ? ` in ${category}` : ''}.
                        </p>
                    )}
                    <div
                        className="module-list"
                        style={{ height: virtualizer.getTotalSize() }}
                    >
                        {virtualItems.map((item) => (
                            <div
                                key={item.key}
                                ref={virtualizer.measureElement}
                                data-index={item.index}
                                className="module-list-item"
                                style={{
                                    transform: `translateY(${item.start}px)`,
                                }}
                            >
                                <ReferenceCard
                                    entry={filteredModules[item.index]}
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
