import type { RefObject } from 'react';
import React, {
    memo,
    useCallback,
    useLayoutEffect,
    useMemo,
    useRef,
    useState,
} from 'react';
import Markdown from 'react-markdown';
import { measureElement, useVirtualizer } from '@tanstack/react-virtual';
import { LinkifyTypes } from './DocsTypeLinks';
import type { DslTypeName } from '../../shared/dsl/typeDocs';
import type {
    EnumVariantInfo,
    Schema,
} from '../../shared/dsl/schemaTypeResolver';
import {
    schemaToTypeExpr,
    getEnumVariants,
} from '../../shared/dsl/schemaTypeResolver';
import { formatOutputMeta, paramMeta } from '../../shared/dsl/paramRange';

const CARD_GAP_PX = 15;
const ESTIMATED_CARD_HEIGHT_PX = 320;

const getSignature = (module: Schema): string => {
    const { positionalArgs } = module;
    const positionalKeys = new Set(positionalArgs.map((a) => a.name));
    const schemaRequired: string[] = module.paramsSchema.required ?? [];

    const parts: string[] = positionalArgs.map((a) =>
        schemaRequired.includes(a.name) ? a.name : `${a.name}?`,
    );

    const configKeys = Object.keys(module.paramsSchema.properties).filter(
        (k) => !positionalKeys.has(k),
    );

    if (configKeys.length > 0) {
        parts.push(
            `{ ${configKeys.map((k) => (schemaRequired.includes(k) ? k : `${k}?`)).join(', ')} }`,
        );
    }

    return `${module.name}(${parts.join(', ')})`;
};

const getParams = (module: Schema) => {
    const properties = Object.entries(module.paramsSchema.properties);
    return properties.map(([name, schema]) => {
        let type: string | undefined;
        try {
            type = schemaToTypeExpr(schema, module.paramsSchema);
        } catch {
            // Fall back to no type annotation for unsupported schemas
        }
        let variants: EnumVariantInfo[] | null = null;
        try {
            variants = getEnumVariants(schema, module.paramsSchema);
        } catch {
            // Fall back to no variant info
        }
        return {
            description: schema.description as string | undefined,
            meta: paramMeta(module, name),
            name,
            type,
            variants,
        };
    });
};

/** A number stays on the line of its unit, e.g. `5 V`. */
const keepUnits = (text: string): string =>
    text.replace(/(\d) (V|Hz|kHz|dB|s|ms)\b/g, '$1\u00a0$2');

/** One-line param, output, or value doc: inline markdown only. */
const InlineDoc: React.FC<{ text: string }> = ({ text }) => (
    <Markdown allowedElements={['code', 'em', 'strong']} unwrapDisallowed>
        {keepUnits(text)}
    </Markdown>
);

const VariantItems: React.FC<{ variants: EnumVariantInfo[] }> = ({
    variants,
}) => (
    <>
        {variants.map((v) => (
            <li key={v.value}>
                <code>{v.rawValue as string}</code>
                {v.description && (
                    <>
                        {' '}
                        &mdash; <InlineDoc text={v.description} />
                    </>
                )}
            </li>
        ))}
    </>
);

interface ModuleCardProps {
    module: Schema;
    onTypeClick: (typeName: DslTypeName) => void;
}

const ModuleCard = memo<ModuleCardProps>(({ module, onTypeClick }) => {
    const params = useMemo(() => getParams(module), [module]);
    return (
        <div className="module-card">
            <h3>
                <code>{getSignature(module)}</code>
            </h3>
            <div className="module-documentation">
                <Markdown>{module.documentation}</Markdown>
            </div>
            <h4>Inputs</h4>
            <ul>
                {params.map(({ name, type, description, meta, variants }) => (
                    <li key={name}>
                        <strong>
                            {name}
                            {type && (
                                <>
                                    {': '}
                                    <LinkifyTypes
                                        text={type}
                                        onTypeClick={onTypeClick}
                                    />
                                </>
                            )}
                        </strong>
                        {description && (
                            <>
                                {' '}
                                &mdash; <InlineDoc text={description} />
                            </>
                        )}
                        {meta && <span className="param-meta">{meta}</span>}
                        {variants &&
                            variants.some((v) => v.description) &&
                            (variants.length > 8 ? (
                                <details className="enum-variants">
                                    <summary>
                                        {variants.length} values (click to
                                        expand)
                                    </summary>
                                    <ul>
                                        <VariantItems variants={variants} />
                                    </ul>
                                </details>
                            ) : (
                                <ul className="enum-variants">
                                    <VariantItems variants={variants} />
                                </ul>
                            ))}
                    </li>
                ))}
            </ul>
            <h4>Outputs</h4>
            <ul>
                {module.outputs.map((out) => {
                    const meta = formatOutputMeta(out);
                    return (
                        <li key={out.name}>
                            <strong>{out.default ? 'main' : out.name}</strong>:{' '}
                            <InlineDoc text={out.description} />
                            {meta && <span className="param-meta">{meta}</span>}
                        </li>
                    );
                })}
            </ul>
        </div>
    );
});
ModuleCard.displayName = 'ModuleCard';

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

    const filteredModules = useMemo(() => {
        const query = searchQuery.toLowerCase();
        // Name matches lead, best first; modules that only mention the query
        // in their docs follow, each group keeping the display order.
        const rankOf = (schema: Schema): number => {
            const name = schema.name.toLowerCase();
            if (name === query) return 0;
            if (name.startsWith(query)) return 1;
            if (name.includes(query)) return 2;
            return schema.documentation.toLowerCase().includes(query) ? 3 : -1;
        };
        return modules
            .map((schema) => ({ rank: rankOf(schema), schema }))
            .filter(({ rank }) => rank >= 0)
            .sort((a, b) => a.rank - b.rank)
            .map(({ schema }) => schema);
    }, [modules, searchQuery]);

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
                        <ModuleCard
                            module={filteredModules[item.index]}
                            onTypeClick={onTypeClick}
                        />
                    </div>
                ))}
            </div>
        </div>
    );
};
