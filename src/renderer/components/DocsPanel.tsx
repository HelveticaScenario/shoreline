import React, { useState, useEffect, useCallback, useRef } from 'react';
import { VideoDocs } from './VideoDocs';
import { LinkifyTypes, TypeLink } from './DocsTypeLinks';
import { ModuleReference } from './ModuleReference';
import type { DocsTarget } from './docsCommand';
import electronAPI from '../electronAPI';
import type {
    DslTypeName,
    TypeDocumentation,
    GlobalFunctionDoc,
} from '../../shared/dsl/typeDocs';
import {
    TYPE_DOCS,
    DSL_TYPE_NAMES,
    isDslType,
    GLOBAL_DOCS,
} from '../../shared/dsl/typeDocs';
import type { Schemas } from '../../shared/dsl/schemaTypeResolver';
import './DocsPanel.css';

type Page = 'types' | 'globals' | 'reference' | 'video';

const PAGES: { id: Page; label: string }[] = [
    { id: 'types', label: 'Types' },
    { id: 'globals', label: 'Globals' },
    { id: 'reference', label: 'Reference' },
    { id: 'video', label: 'Video' },
];

const MODULE_NAME_ORDER = new Intl.Collator('en', {
    numeric: true,
    sensitivity: 'base',
});

const NO_MODULES: Schemas = [];

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
    const [activePage, setActivePage] = useState<Page>('types');
    const [searchQuery, setSearchQuery] = useState('');
    const [category, setCategory] = useState<string | null>(null);
    const [schemas, setSchemas] = useState<Schemas>();
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
                setSchemas(
                    schemaList
                        .filter((e) => !e.name.startsWith('_'))
                        .sort((a, b) =>
                            MODULE_NAME_ORDER.compare(a.name, b.name),
                        ),
                );
            })
            .catch(console.error);
    }, []);

    useEffect(() => {
        if (!target) {
            return;
        }
        const { symbolType, name } = target;
        if (symbolType === 'type' && isDslType(name)) {
            setActivePage('types');
            setRevealedType({ name });
            setExpandedTypes((prev) => new Set(prev).add(name));
        } else if (symbolType === 'module' || symbolType === 'namespace') {
            setActivePage('reference');
            setSearchQuery(name);
            setCategory(null);
        }
    }, [target]);

    // Handle type link clicks - navigate to types page and expand the type
    const handleTypeClick = useCallback((typeName: DslTypeName) => {
        setActivePage('types');
        setRevealedType({ name: typeName });
        setExpandedTypes((prev) => new Set(prev).add(typeName));
    }, []);

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
            case 'globals': {
                const groups = GLOBAL_DOCS.reduce<
                    Record<string, GlobalFunctionDoc[]>
                >((acc, fn) => {
                    (acc[fn.group] ??= []).push(fn);
                    return acc;
                }, {});
                return (
                    <div>
                        <h2>Global Functions</h2>
                        <p className="types-intro">
                            These functions are available in every patch without
                            any imports.
                        </p>
                        {Object.entries(groups).map(([group, fns]) => (
                            <div key={group}>
                                <h3>{group}</h3>
                                {fns.map((fn) => (
                                    <div key={fn.name} className="module-card">
                                        <h4>
                                            <code>{fn.signature}</code>
                                        </h4>
                                        <p>{fn.description}</p>
                                        {fn.params && fn.params.length > 0 && (
                                            <>
                                                <h5>Parameters</h5>
                                                <ul>
                                                    {fn.params.map((p) => (
                                                        <li key={p.name}>
                                                            <strong>
                                                                {p.name}:{' '}
                                                                <LinkifyTypes
                                                                    text={
                                                                        p.type
                                                                    }
                                                                    onTypeClick={
                                                                        handleTypeClick
                                                                    }
                                                                />
                                                            </strong>{' '}
                                                            &mdash;{' '}
                                                            {p.description}
                                                        </li>
                                                    ))}
                                                </ul>
                                            </>
                                        )}
                                        {fn.examples.length > 0 && (
                                            <>
                                                <h5>Example</h5>
                                                <pre>
                                                    {fn.examples.join('\n\n')}
                                                </pre>
                                            </>
                                        )}
                                    </div>
                                ))}
                            </div>
                        ))}
                    </div>
                );
            }
            case 'video':
                return <VideoDocs />;
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
            case 'reference':
                return (
                    <ModuleReference
                        modules={schemas ?? NO_MODULES}
                        searchQuery={searchQuery}
                        onSearchQueryChange={setSearchQuery}
                        category={category}
                        onCategoryChange={setCategory}
                        onTypeClick={handleTypeClick}
                    />
                );
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
                    activePage === 'reference'
                        ? 'docs-content docs-content-split'
                        : 'docs-content'
                }
            >
                {renderContent()}
            </div>
        </div>
    );
};
