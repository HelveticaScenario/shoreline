import type { RefObject } from 'react';
import React, {
    useCallback,
    useLayoutEffect,
    useMemo,
    useRef,
    useState,
} from 'react';
import { measureElement, useVirtualizer } from '@tanstack/react-virtual';
import { ReferenceCard } from './ReferenceCard';
import type { DslTypeName } from '../../shared/dsl/typeDocs';
import type { Schema } from '../../shared/dsl/schemaTypeResolver';
import type { ReferenceEntry } from '../../shared/dsl/referenceDocs';
import { moduleToReferenceEntry } from '../../shared/dsl/referenceDocs';

const CARD_GAP_PX = 15;
const ESTIMATED_CARD_HEIGHT_PX = 320;

interface ModuleReferenceProps {
    /** Already sorted in display order. */
    modules: Schema[];
    searchQuery: string;
    onSearchQueryChange: (query: string) => void;
    scrollRef: RefObject<HTMLDivElement | null>;
    onTypeClick: (typeName: DslTypeName) => void;
}

/**
 * Module cards rendered through a virtualizer that scrolls the docs pane, so
 * only the cards near the viewport are mounted.
 */
export const ModuleReference: React.FC<ModuleReferenceProps> = ({
    modules,
    searchQuery,
    onSearchQueryChange,
    scrollRef,
    onTypeClick,
}) => {
    const listRef = useRef<HTMLDivElement>(null);
    // The list's offset within the scrolling pane, below the heading and search box.
    const [scrollMargin, setScrollMargin] = useState(0);

    const entries = useMemo(
        () => modules.map(moduleToReferenceEntry),
        [modules],
    );

    const filteredModules = useMemo(() => {
        const query = searchQuery.toLowerCase();
        // Name matches lead, best first; then entries whose summary or
        // description mention the query; then those whose params or examples
        // do. Each group keeps the display order.
        const rankOf = (entry: ReferenceEntry): number => {
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
                ...[
                    ...examples,
                    ...sections.flatMap((s) => s.examples),
                ].flatMap((e) => [e.title, e.code]),
            ];
            return rest.join('\n').toLowerCase().includes(query) ? 4 : -1;
        };
        return entries
            .map((entry) => ({ entry, rank: rankOf(entry) }))
            .filter(({ rank }) => rank >= 0)
            .sort((a, b) => a.rank - b.rank)
            .map(({ entry }) => entry);
    }, [entries, searchQuery]);

    useLayoutEffect(() => {
        const scrollElement = scrollRef.current;
        const list = listRef.current;
        if (!scrollElement || !list) {
            return;
        }
        const measure = () => {
            // A hidden pane has no layout; keep the margin it had while shown.
            if (scrollElement.clientHeight === 0) {
                return;
            }
            setScrollMargin(
                list.getBoundingClientRect().top -
                    scrollElement.getBoundingClientRect().top +
                    scrollElement.scrollTop,
            );
        };
        measure();
        // The heading and search box above the list can change height without the pane resizing.
        const observer = new ResizeObserver(measure);
        observer.observe(scrollElement);
        for (const sibling of list.parentElement?.children ?? []) {
            if (sibling !== list) {
                observer.observe(sibling);
            }
        }
        return () => observer.disconnect();
    }, [scrollRef]);

    const getItemKey = useCallback(
        (index: number) => filteredModules[index].name,
        [filteredModules],
    );

    const virtualizer = useVirtualizer({
        count: filteredModules.length,
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
        scrollMargin,
        useFlushSync: false,
    });

    // A new query starts its results from the first match. Scrolling through the
    // virtualizer keeps its offset in step, so re-measuring cannot shift the pane back.
    useLayoutEffect(() => {
        virtualizer.scrollToOffset(0);
    }, [virtualizer, searchQuery]);

    return (
        <div>
            <h2>Module Reference</h2>
            <div className="search-container">
                <input
                    type="text"
                    placeholder="Search modules..."
                    value={searchQuery}
                    onChange={(e) => onSearchQueryChange(e.target.value)}
                    className="search-input"
                />
            </div>
            <div
                ref={listRef}
                className="module-list"
                style={{ height: virtualizer.getTotalSize() }}
            >
                {virtualizer.getVirtualItems().map((item) => (
                    <div
                        key={item.key}
                        ref={virtualizer.measureElement}
                        data-index={item.index}
                        className="module-list-item"
                        style={{
                            transform: `translateY(${item.start - scrollMargin}px)`,
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
    );
};
