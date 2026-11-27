import { createRoot } from 'react-dom/client';
import Markdown from 'react-markdown';

interface MarkupContent {
    kind?: string;
    value: string;
}

/** The text of an LSP documentation value, which is markdown or plain. */
export function markupText(
    content: MarkupContent | string | (MarkupContent | string)[] | undefined,
): string {
    if (content === undefined) {
        return '';
    }
    if (Array.isArray(content)) {
        return content.map(markupText).join('\n\n');
    }
    return typeof content === 'string' ? content : content.value;
}

/** A detached element showing `markdown`, for a tooltip's content. */
export function markdownElement(
    markdown: string,
    className: string,
): HTMLElement {
    const dom = document.createElement('div');
    dom.className = className;
    createRoot(dom).render(<Markdown>{markdown}</Markdown>);
    return dom;
}
