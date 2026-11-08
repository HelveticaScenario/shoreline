import type { RefObject } from 'react';
import React, { memo, useLayoutEffect, useMemo, useRef, useState } from 'react';
import Markdown from 'react-markdown';
import { useVirtualizer } from '@tanstack/react-virtual';
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
            name,
            type,
            variants,
        };
    });
};

const VariantItems: React.FC<{ variants: EnumVariantInfo[] }> = ({
    variants,
}) => (
    <>
        {variants.map((v) => (
            <li key={v.value}>
                <code>{v.rawValue as string}</code>
                {v.description && <> &mdash; {v.description}</>}
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
                {params.map(({ name, type, description, variants }) => (
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
                        {description && <> &mdash; {description}</>}
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
                {module.outputs.map((out) => (
                    <li key={out.name}>
                        <strong>{out.default ? 'main' : out.name}</strong>:{' '}
                        <LinkifyTypes
                            text={out.description}
                            onTypeClick={onTypeClick}
                        />
                    </li>
                ))}
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
        return modules.filter(
            (schema) =>
                schema.name.toLowerCase().includes(query) ||
                schema.documentation.toLowerCase().includes(query),
        );
    }, [modules, searchQuery]);

    useLayoutEffect(() => {
        const scrollElement = scrollRef.current;
        const list = listRef.current;
        if (!scrollElement || !list) {
            return;
        }
        const measure = () =>
            setScrollMargin(
                list.getBoundingClientRect().top -
                    scrollElement.getBoundingClientRect().top +
                    scrollElement.scrollTop,
            );
        measure();
        // The pane reports zero size while the sidebar hides it, so re-measure when it reappears.
        const observer = new ResizeObserver(measure);
        observer.observe(scrollElement);
        return () => observer.disconnect();
    }, [scrollRef]);

    const virtualizer = useVirtualizer({
        count: filteredModules.length,
        estimateSize: () => ESTIMATED_CARD_HEIGHT_PX,
        gap: CARD_GAP_PX,
        getItemKey: (index) => filteredModules[index].name,
        getScrollElement: () => scrollRef.current,
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
