import React, { useEffect, useState } from 'react';
import { useCustomMonaco } from '../hooks/useCustomMonaco';
import { useTheme } from '../themes/useTheme';
import type { AppTheme } from '../themes/types';
import { applyMonacoTheme } from './monaco/theme';

/** Colorized HTML by theme and source, so code scrolled or opened
 *  back into view keeps its colours without a flash of plain text. Monaco's
 *  token classes index the active theme's colours, so each theme has its own
 *  entries. */
const colorized = new Map<string, string>();

/** The app theme this module last gave Monaco. Monaco's theme is global, and
 *  code can be colorized while no editor is mounted to have set it. */
let appliedTheme: AppTheme | null = null;

interface ColorizedCodeProps {
    code: string;
    /** A `<code>` inside running text rather than a `<pre>` block. */
    inline?: boolean;
}

/** Code in the editor's colours, plain until Monaco has tokenized it. */
export const ColorizedCode: React.FC<ColorizedCodeProps> = ({
    code,
    inline = false,
}) => {
    const monaco = useCustomMonaco();
    const { theme } = useTheme();
    const key = `${theme.id}\n${code}`;
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
        const timer = setTimeout(() => {
            if (appliedTheme !== theme) {
                applyMonacoTheme(monaco, theme, `theme-${theme.id}`);
                appliedTheme = theme;
            }
            monaco.editor
                .colorize(code, 'javascript', { tabSize: 2 })
                .then((result) => {
                    // Plain spaces, so copied code parses as it reads. Every
                    // line ends in a `<br/>`; the last one would break an
                    // inline snippet out of its line.
                    const plain = result
                        .replace(/&nbsp;|\u00a0/g, ' ')
                        .replace(/<br\/>$/, '');
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
    }, [monaco, code, key, theme]);
    const content = html ? (
        // Monaco escapes the source it tokenizes.
        <code dangerouslySetInnerHTML={{ __html: html }} />
    ) : (
        <code>{code}</code>
    );
    return inline ? content : <pre>{content}</pre>;
};
