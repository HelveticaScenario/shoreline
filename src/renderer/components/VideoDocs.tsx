import React from 'react';
import {
    VIDEO_CHAIN,
    VIDEO_CHAIN_EXAMPLES,
    VIDEO_CHAIN_INTRO,
    VIDEO_DOCS,
    VIDEO_GROUPS,
    VIDEO_INTRO,
} from '../../shared/dsl/videoDocs';
import type { VideoDocParam } from '../../shared/dsl/videoDocs';
import { useQuoteExample } from '../hooks/useQuoteExample';

/** Docs text with its `code` spans rendered. */
const withCodeSpans = (text: string) =>
    text
        .split(/(`[^`]+`)/)
        .map((part, i) =>
            part.startsWith('`') ? (
                <code key={i}>{part.slice(1, -1)}</code>
            ) : (
                part
            ),
        );

const Prose: React.FC<{ text: string }> = ({ text }) => (
    <>
        {text.split('\n\n').map((paragraph) => (
            <p key={paragraph}>{withCodeSpans(paragraph)}</p>
        ))}
    </>
);

const ParamList: React.FC<{ params: VideoDocParam[] }> = ({ params }) => (
    <ul>
        {params.map((param) => (
            <li key={param.name}>
                <strong>{param.name}</strong> &mdash;{' '}
                {withCodeSpans(param.description)}
            </li>
        ))}
    </ul>
);

/** The Docs panel's Video page, rendered from {@link VIDEO_DOCS}. */
export const VideoDocs: React.FC = () => {
    const quote = useQuoteExample();
    const examples = (codes: readonly string[]) =>
        codes.map(quote).join('\n\n');
    return (
        <div>
            <h2>Video</h2>
            <div className="types-intro">
                <Prose text={VIDEO_INTRO.description} />
                <pre>{examples(VIDEO_INTRO.examples)}</pre>
            </div>
            <h3>Chaining</h3>
            <div className="module-card">
                <Prose text={VIDEO_CHAIN_INTRO} />
                <pre>{examples(VIDEO_CHAIN_EXAMPLES)}</pre>
                <h5>Methods</h5>
                <ul>
                    {VIDEO_CHAIN.map((method) => (
                        <li key={method.name}>
                            <strong>.{method.name}</strong> &mdash;{' '}
                            {method.description}
                            {method.params && method.params.length > 0 && (
                                <ParamList params={method.params} />
                            )}
                        </li>
                    ))}
                </ul>
            </div>
            {VIDEO_GROUPS.map((group) => (
                <div key={group}>
                    <h3>{group}</h3>
                    {VIDEO_DOCS.filter((doc) => doc.group === group).map(
                        (doc) => (
                            <div key={doc.name} className="module-card">
                                <h4>
                                    <code>$v.{doc.name}</code>
                                </h4>
                                <pre>{doc.declarations.join('\n')}</pre>
                                <Prose text={doc.description} />
                                {doc.params.length > 0 && (
                                    <>
                                        <h5>Parameters</h5>
                                        <ParamList params={doc.params} />
                                    </>
                                )}
                                {doc.examples.length > 0 && (
                                    <>
                                        <h5>Example</h5>
                                        <pre>{examples(doc.examples)}</pre>
                                    </>
                                )}
                            </div>
                        ),
                    )}
                </div>
            ))}
        </div>
    );
};
