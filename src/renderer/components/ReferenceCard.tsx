import React, { memo, useEffect, useState } from 'react';
import Markdown from 'react-markdown';
import { LinkifyTypes } from './DocsTypeLinks';
import { useCustomMonaco } from '../hooks/useCustomMonaco';
import { useTheme } from '../themes/ThemeContext';
import type { DslTypeName } from '../../shared/dsl/typeDocs';
import type { EnumVariantInfo } from '../../shared/dsl/schemaTypeResolver';
import type {
    DocExample,
    DocTable,
    ReferenceEntry,
} from '../../shared/dsl/referenceDocs';

/** A number stays on the line of its unit, e.g. `5 V`. */
const keepUnits = (text: string): string =>
    text.replace(/(\d) (V|Hz|kHz|dB|s|ms)\b/g, '$1 $2');

/** Inline markdown: code, emphasis and links, never blocks. Links open in
 *  the browser, through the main window's window-open handler. */
const InlineDoc: React.FC<{ text: string }> = ({ text }) => (
    <Markdown
        allowedElements={['code', 'em', 'strong', 'a']}
        unwrapDisallowed
        components={{
            a: ({ href, children }) => (
                <a href={href} target="_blank" rel="noreferrer">
                    {children}
                </a>
            ),
        }}
    >
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

/**
 * A real table when the card is wide; below the container breakpoint each
 * row stacks, its first cell as the heading and the rest as `column: value`
 * lines (see `.doc-table` in DocsPanel.css).
 */
const DocTableView: React.FC<{ table: DocTable }> = ({ table }) => (
    <table className="doc-table">
        <thead>
            <tr>
                {table.columns.map((c) => (
                    <th key={c}>
                        <InlineDoc text={c} />
                    </th>
                ))}
            </tr>
        </thead>
        <tbody>
            {table.rows.map((row, r) => (
                <tr key={r}>
                    {row.map((cell, c) => (
                        <td key={c} data-label={table.columns[c]}>
                            <InlineDoc text={cell} />
                        </td>
                    ))}
                </tr>
            ))}
        </tbody>
    </table>
);

/** Colorized HTML by theme and source, so a card scrolled back into view
 *  keeps its colours without a flash of plain text. Monaco's token classes
 *  index the active theme's colours, so each theme has its own entries. */
const colorized = new Map<string, string>();

/** An example in the editor's colours, plain until Monaco has tokenized it. */
const ExampleCode: React.FC<{ code: string }> = ({ code }) => {
    const monaco = useCustomMonaco();
    const themeId = useTheme().theme.id;
    const key = `${themeId}\n${code}`;
    const [colored, setColored] = useState(() => {
        const html = colorized.get(key);
        return html === undefined ? undefined : { html, key };
    });
    const html = colored?.key === key ? colored.html : colorized.get(key);
    useEffect(() => {
        if (!monaco || colorized.has(key)) {
            return;
        }
        let live = true;
        // The editor applies a new theme in its own effect; tokenizing a
        // task later reads that theme's colours.
        const timer = setTimeout(() => {
            monaco.editor
                .colorize(code, 'javascript', { tabSize: 2 })
                .then((result) => {
                    // Plain spaces, so copied code parses as it reads.
                    const plain = result.replace(/&nbsp;|\u00a0/g, ' ');
                    colorized.set(key, plain);
                    if (live) {
                        setColored({ html: plain, key });
                    }
                })
                .catch(() => {
                    // The plain code stays.
                });
        });
        return () => {
            live = false;
            clearTimeout(timer);
        };
    }, [monaco, code, key]);
    return (
        <pre>
            {html ? (
                // Monaco escapes the source it tokenizes.
                <code dangerouslySetInnerHTML={{ __html: html }} />
            ) : (
                <code>{code}</code>
            )}
        </pre>
    );
};

const Examples: React.FC<{ examples: DocExample[] }> = ({ examples }) => (
    <>
        {examples.map((e) => (
            <figure key={e.title} className="doc-example">
                <figcaption>{e.title}</figcaption>
                <ExampleCode code={e.code} />
            </figure>
        ))}
    </>
);

interface ReferenceCardProps {
    entry: ReferenceEntry;
    onTypeClick: (typeName: DslTypeName) => void;
}

/** The full Reference-pane view of one entry. */
export const ReferenceCard = memo<ReferenceCardProps>(
    ({ entry, onTypeClick }) => {
        const { docs } = entry;
        return (
            <div className="module-card">
                <h3>
                    <code>{entry.signature}</code>
                </h3>
                <p className="doc-summary">{docs.summary}</p>
                <div className="module-documentation">
                    <p>
                        <InlineDoc text={docs.description} />
                    </p>
                </div>
                {entry.params.length > 0 && (
                    <>
                        <h4>Inputs</h4>
                        <ul>
                            {entry.params.map((p) => (
                                <li key={p.name}>
                                    <strong>
                                        {p.name}
                                        {p.type && (
                                            <>
                                                {': '}
                                                <LinkifyTypes
                                                    text={p.type}
                                                    onTypeClick={onTypeClick}
                                                />
                                            </>
                                        )}
                                    </strong>
                                    {p.description && (
                                        <>
                                            {' '}
                                            &mdash;{' '}
                                            <InlineDoc text={p.description} />
                                        </>
                                    )}
                                    {p.meta && (
                                        <span className="param-meta">
                                            {p.meta}
                                        </span>
                                    )}
                                    {p.variants &&
                                        (p.variants.length > 8 ? (
                                            <details className="enum-variants">
                                                <summary>
                                                    {p.variants.length} values
                                                    (click to expand)
                                                </summary>
                                                <ul>
                                                    <VariantItems
                                                        variants={p.variants}
                                                    />
                                                </ul>
                                            </details>
                                        ) : (
                                            <ul className="enum-variants">
                                                <VariantItems
                                                    variants={p.variants}
                                                />
                                            </ul>
                                        ))}
                                </li>
                            ))}
                        </ul>
                    </>
                )}
                {entry.outputs && entry.outputs.length > 0 && (
                    <>
                        <h4>Outputs</h4>
                        <ul>
                            {entry.outputs.map((out) => (
                                <li key={out.name}>
                                    <strong>{out.name}</strong>
                                    {out.main && ' (default)'}
                                    : <InlineDoc text={out.description} />
                                    {out.meta && (
                                        <span className="param-meta">
                                            {out.meta}
                                        </span>
                                    )}
                                </li>
                            ))}
                        </ul>
                    </>
                )}
                {docs.sections.map((section) => (
                    <section key={section.title} className="doc-section">
                        <h4>{section.title}</h4>
                        {section.body && (
                            <p>
                                <InlineDoc text={section.body} />
                            </p>
                        )}
                        {section.table && (
                            <DocTableView table={section.table} />
                        )}
                        <Examples examples={section.examples} />
                    </section>
                ))}
                {docs.examples.length > 0 && (
                    <section className="doc-section">
                        <h4>
                            {docs.examples.length > 1 ? 'Examples' : 'Example'}
                        </h4>
                        <Examples examples={docs.examples} />
                    </section>
                )}
            </div>
        );
    },
);
ReferenceCard.displayName = 'ReferenceCard';
