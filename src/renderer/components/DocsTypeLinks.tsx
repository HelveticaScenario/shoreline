import type { ReactNode } from 'react';
import React from 'react';
import type { DslTypeName } from '../../shared/dsl/typeDocs';
import { DSL_TYPE_NAMES } from '../../shared/dsl/typeDocs';

/**
 * Regex pattern matching all DSL type names for linkification.
 * Sorted longest-first so `Poly<Signal>` matches before `Signal`.
 * Uses lookaround instead of \b since type names contain non-word chars (<>).
 */
const SORTED_TYPE_NAMES = [...DSL_TYPE_NAMES].sort(
    (a, b) => b.length - a.length,
);
const TYPE_PATTERN_SOURCE = `(?<!\\w)(${SORTED_TYPE_NAMES.join('|')})(?!\\w)`;

interface TypeLinkProps {
    typeName: DslTypeName;
    onTypeClick: (typeName: DslTypeName) => void;
}

/**
 * A clickable link that navigates to a type's documentation.
 */
export const TypeLink: React.FC<TypeLinkProps> = ({
    typeName,
    onTypeClick,
}) => (
    <button
        className="type-link"
        onClick={() => onTypeClick(typeName)}
        title={`View ${typeName} documentation`}
    >
        {typeName}
    </button>
);

interface LinkifyTypesProps {
    text: string;
    onTypeClick: (typeName: DslTypeName) => void;
}

/**
 * Component that renders text with DSL type names as clickable links.
 */
export const LinkifyTypes: React.FC<LinkifyTypesProps> = ({
    text,
    onTypeClick,
}) => {
    const parts: ReactNode[] = [];
    let lastIndex = 0;
    let match: RegExpExecArray | null;
    let key = 0;

    const typePattern = new RegExp(TYPE_PATTERN_SOURCE, 'g');

    while ((match = typePattern.exec(text)) !== null) {
        // Add text before the match
        if (match.index > lastIndex) {
            parts.push(text.slice(lastIndex, match.index));
        }
        // Add the type link
        const typeName = match[1] as DslTypeName;
        parts.push(
            <TypeLink
                key={key++}
                typeName={typeName}
                onTypeClick={onTypeClick}
            />,
        );
        lastIndex = match.index + match[0].length;
    }

    // Add remaining text
    if (lastIndex < text.length) {
        parts.push(text.slice(lastIndex));
    }

    return <>{parts}</>;
};
