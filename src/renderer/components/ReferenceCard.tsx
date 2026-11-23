import React, { memo } from 'react';
import Markdown from 'react-markdown';
import { LinkifyTypes } from './DocsTypeLinks';
import { ColorizedCode } from './ColorizedCode';
import { useQuoteExample } from '../hooks/useQuoteExample';
import { EXPERIMENTAL_NOTICE } from '../../shared/dsl/experimental';
import type { DslTypeName } from '../../shared/dsl/typeDocs';
import type { EnumVariantInfo } from '../../shared/dsl/schemaTypeResolver';
import type {
    DocExample,
    DocTable,
    ReferenceEntry,
    ValueDoc,
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

const Examples: React.FC<{ examples: DocExample[] }> = ({ examples }) => {
    const quote = useQuoteExample();
    return (
        <>
            {examples.map((e) => (
                <figure key={e.title} className="doc-example">
                    <figcaption>{e.title}</figcaption>
                    <ColorizedCode code={quote(e.code)} />
                </figure>
            ))}
        </>
    );
};

const Meta: React.FC<{ meta?: string }> = ({ meta }) =>
    meta ? <span className="param-meta">{meta}</span> : null;

/** An output or a returned member: its name, a note such as `(default)`,
 *  its description and range. */
const ValueItem: React.FC<{ value: ValueDoc; note?: string }> = ({
    value,
    note,
}) => (
    <li>
        <strong>{value.name}</strong>
        {note}: <InlineDoc text={value.description} />
        <Meta meta={value.meta} />
    </li>
);

interface ReferenceCardProps {
    entry: ReferenceEntry;
    onTypeClick: (typeName: DslTypeName) => void;
}

/**
 * The full Docs-panel view of one module or global. The virtualized list that
 * mounts the cards renders on every scroll and is not compiled, so `memo`
 * keeps a scroll from re-rendering them.
 */
export const ReferenceCard = memo<ReferenceCardProps>(
    ({ entry, onTypeClick }) => {
        const { docs } = entry;
        return (
            <div className="module-card">
                <h3>
                    <code>{entry.signature}</code>
                </h3>
                <p className="doc-summary">{docs.summary}</p>
                {entry.experimental && (
                    <p className="doc-experimental">
                        <InlineDoc text={EXPERIMENTAL_NOTICE} />
                    </p>
                )}
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
                                        {p.rest && '...'}
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
                                    <Meta meta={p.meta} />
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
                                <ValueItem
                                    key={out.name}
                                    value={out}
                                    note={out.main ? ' (default)' : undefined}
                                />
                            ))}
                        </ul>
                    </>
                )}
                {entry.returns && (
                    <>
                        <h4>Returns</h4>
                        <p>
                            <strong>
                                <LinkifyTypes
                                    text={entry.returns.type}
                                    onTypeClick={onTypeClick}
                                />
                            </strong>{' '}
                            &mdash;{' '}
                            <InlineDoc text={entry.returns.description} />
                            <Meta meta={entry.returns.meta} />
                        </p>
                        {entry.returns.members && (
                            <ul>
                                {entry.returns.members.map((member) => (
                                    <ValueItem
                                        key={member.name}
                                        value={member}
                                    />
                                ))}
                            </ul>
                        )}
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
