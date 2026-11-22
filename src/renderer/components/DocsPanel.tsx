import React, { useState, useEffect, useCallback, useRef } from 'react';
import { VideoDocs } from './VideoDocs';
import { BuiltinDocs } from './BuiltinDocs';
import { LinkifyTypes, TypeLink } from './DocsTypeLinks';
import { ReferenceBrowser } from './ReferenceBrowser';
import type { DocsTarget } from './docsCommand';
import electronAPI from '../electronAPI';
import type { DslTypeName, TypeDocumentation } from '../../shared/dsl/typeDocs';
import {
    TYPE_DOCS,
    DSL_TYPE_NAMES,
    isDslType,
} from '../../shared/dsl/typeDocs';
import type { ReferenceEntry } from '../../shared/dsl/referenceDocs';
import { moduleToReferenceEntry } from '../../shared/dsl/referenceDocs';
import { GLOBAL_ENTRIES } from '../../shared/dsl/globalDocs';
import './DocsPanel.css';

/** The pages that browse reference entries. */
type BrowserPage = 'modules' | 'globals';
type Page = BrowserPage | 'types' | 'video' | 'builtins';

const PAGES: { id: Page; label: string }[] = [
    { id: 'modules', label: 'Modules' },
    { id: 'types', label: 'Types' },
    { id: 'globals', label: 'Globals' },
    { id: 'video', label: 'Video' },
    { id: 'builtins', label: 'Built-ins' },
];

const NAME_ORDER = new Intl.Collator('en', {
    numeric: true,
    sensitivity: 'base',
});

const byName = (a: ReferenceEntry, b: ReferenceEntry) =>
    NAME_ORDER.compare(a.name, b.name);

const SORTED_GLOBALS = [...GLOBAL_ENTRIES].sort(byName);

const isBrowserPage = (page: Page): page is BrowserPage =>
    page === 'modules' || page === 'globals';

interface BrowserFilter {
    searchQuery: string;
    category: string | null;
}

const NO_FILTERS: Record<BrowserPage, BrowserFilter> = {
    globals: { category: null, searchQuery: '' },
    modules: { category: null, searchQuery: '' },
};

interface DocsPanelProps {
    target: DocsTarget | null;
    hidden: boolean;
}

interface TypeCardProps {
    typeDoc: TypeDocumentation;
    isExpanded: boolean;
    onToggle: () => void;
    onTypeClick: (typeName: DslTypeName) => void;
}

/**
 * Card displaying documentation for a single DSL type.
 */
const TypeCard: React.FC<TypeCardProps> = ({
    typeDoc,
    isExpanded,
    onToggle,
    onTypeClick,
}) => (
    <div className={`type-card ${isExpanded ? 'expanded' : ''}`}>
        <div className="type-card-header" onClick={onToggle}>
            <h3>{typeDoc.name}</h3>
            <span className="expand-icon">{isExpanded ? '▼' : '▶'}</span>
        </div>

        <p className="type-description">
            <LinkifyTypes
                text={typeDoc.description}
                onTypeClick={onTypeClick}
            />
        </p>

        {isExpanded && (
            <div className="type-details">
                {typeDoc.definition && (
                    <div className="type-definition">
                        <h4>Definition</h4>
                        <code>{typeDoc.definition}</code>
                    </div>
                )}

                {typeDoc.examples.length > 0 && (
                    <div className="type-examples">
                        <h4>Examples</h4>
                        <pre>{typeDoc.examples.join('\n')}</pre>
                    </div>
                )}

                {typeDoc.methods && typeDoc.methods.length > 0 && (
                    <div className="type-methods">
                        <h4>Methods</h4>
                        {typeDoc.methods.map((method) => (
                            <div key={method.name} className="method-card">
                                <code className="method-signature">
                                    <LinkifyTypes
                                        text={method.signature}
                                        onTypeClick={onTypeClick}
                                    />
                                </code>
                                <p>
                                    <LinkifyTypes
                                        text={method.description}
                                        onTypeClick={onTypeClick}
                                    />
                                </p>
                                {method.example && (
                                    <pre className="method-example">
                                        {method.example}
                                    </pre>
                                )}
                            </div>
                        ))}
                    </div>
                )}

                {typeDoc.seeAlso.length > 0 && (
                    <div className="type-see-also">
                        <h4>See Also</h4>
                        <div className="see-also-links">
                            {typeDoc.seeAlso.map((typeName) =>
                                isDslType(typeName) ? (
                                    <TypeLink
                                        key={typeName}
                                        typeName={typeName}
                                        onTypeClick={onTypeClick}
                                    />
                                ) : (
                                    <span key={typeName}>{typeName}</span>
                                ),
                            )}
                        </div>
                    </div>
                )}
            </div>
        )}
    </div>
);

export const DocsPanel: React.FC<DocsPanelProps> = ({ target, hidden }) => {
    const [activePage, setActivePage] = useState<Page>('modules');
    const [filters, setFilters] = useState(NO_FILTERS);
    const [moduleEntries, setModuleEntries] = useState<ReferenceEntry[]>([]);
    // A fresh object per reveal, so revealing the same type again scrolls back to it.
    const [revealedType, setRevealedType] = useState<{
        name: DslTypeName;
    } | null>(null);
    const contentRef = useRef<HTMLDivElement>(null);
    const [expandedTypes, setExpandedTypes] = useState(new Set<DslTypeName>());

    // Fetch schemas once at mount
    useEffect(() => {
        electronAPI
            .getSchemas()
            .then((schemaList) => {
                // Modules named with a leading `_` are internal; hide them from user-facing docs
                setModuleEntries(
                    schemaList
                        .filter((e) => !e.name.startsWith('_'))
                        .map(moduleToReferenceEntry)
                        .sort(byName),
                );
            })
            .catch(console.error);
    }, []);

    // Opens the types page at a type, expanded
    const handleTypeClick = useCallback((typeName: DslTypeName) => {
        setActivePage('types');
        setRevealedType({ name: typeName });
        setExpandedTypes((prev) => new Set(prev).add(typeName));
    }, []);

    // Each new target is revealed in the render that receives it.
    const [revealedTarget, setRevealedTarget] = useState<DocsTarget | null>(
        null,
    );
    if (target !== revealedTarget) {
        setRevealedTarget(target);
        const name = target?.name ?? '';
        if (target?.symbolType === 'type') {
            if (isDslType(name)) {
                handleTypeClick(name);
            }
        } else if (target) {
            const page = target.symbolType === 'global' ? 'globals' : 'modules';
            setActivePage(page);
            setFilters((prev) => ({
                ...prev,
                [page]: { category: null, searchQuery: name },
            }));
        }
    }

    // Toggle type card expansion
    const toggleTypeExpanded = useCallback((typeName: DslTypeName) => {
        setExpandedTypes((prev) => {
            const next = new Set(prev);
            if (next.has(typeName)) {
                next.delete(typeName);
            } else {
                next.add(typeName);
            }
            return next;
        });
    }, []);

    // Scroll to selected type when it changes
    useEffect(() => {
        const container = contentRef.current;
        if (!container || !revealedType || activePage !== 'types') {
            return;
        }
        const element = document.getElementById(`type-${revealedType.name}`);
        if (element) {
            // Scroll the docs pane alone; the app containers around it must not move.
            const offset =
                element.getBoundingClientRect().top -
                container.getBoundingClientRect().top;
            container.scrollTo({
                behavior: 'smooth',
                top: container.scrollTop + offset,
            });
        }
    }, [revealedType, activePage]);

    const renderContent = () => {
        switch (activePage) {
            case 'video':
                return <VideoDocs />;
            case 'builtins':
                return <BuiltinDocs />;
            case 'types':
                return (
                    <div>
                        <h2>Type Reference</h2>
                        <p className="types-intro">
                            These are the core types used in the modular DSL.
                            Click on any type name throughout the documentation
                            to navigate to its definition.
                        </p>

                        <div className="types-grid">
                            {DSL_TYPE_NAMES.map((typeName) => {
                                const typeDoc = TYPE_DOCS[typeName];
                                return (
                                    <div key={typeName} id={`type-${typeName}`}>
                                        <TypeCard
                                            typeDoc={typeDoc}
                                            isExpanded={expandedTypes.has(
                                                typeName,
                                            )}
                                            onToggle={() =>
                                                toggleTypeExpanded(typeName)
                                            }
                                            onTypeClick={handleTypeClick}
                                        />
                                    </div>
                                );
                            })}
                        </div>
                    </div>
                );
            case 'modules':
            case 'globals': {
                const page = activePage;
                const filter = filters[page];
                const setFilter = (change: Partial<BrowserFilter>) =>
                    setFilters((prev) => ({
                        ...prev,
                        [page]: { ...prev[page], ...change },
                    }));
                return (
                    <ReferenceBrowser
                        key={page}
                        entries={
                            page === 'modules' ? moduleEntries : SORTED_GLOBALS
                        }
                        noun={page}
                        searchQuery={filter.searchQuery}
                        onSearchQueryChange={(searchQuery) =>
                            setFilter({ searchQuery })
                        }
                        category={filter.category}
                        onCategoryChange={(category) => setFilter({ category })}
                        onTypeClick={handleTypeClick}
                    />
                );
            }
        }
    };

    return (
        <div className="docs-panel" hidden={hidden}>
            <nav className="docs-pages" aria-label="Docs pages">
                {PAGES.map(({ id, label }) => (
                    <button
                        key={id}
                        aria-pressed={activePage === id}
                        className={activePage === id ? 'active' : ''}
                        onClick={() => setActivePage(id)}
                    >
                        {label}
                    </button>
                ))}
            </nav>
            <div
                ref={contentRef}
                className={
                    isBrowserPage(activePage)
                        ? 'docs-content docs-content-split'
                        : 'docs-content'
                }
            >
                {renderContent()}
            </div>
        </div>
    );
};
