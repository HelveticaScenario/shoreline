import React, { useEffect, useRef, useState } from 'react';
import { Check, Copy } from 'lucide-react';
import electronAPI from '../electronAPI';
import { useCustomMonaco } from '../hooks/useCustomMonaco';
import { useNearScreen } from '../hooks/useNearScreen';
import { ColorizedCode } from './ColorizedCode';
import { ExampleEditor } from './ExampleEditor';
import './CodeExample.css';

/** How long the button shows that the copy happened. */
const COPIED_MS = 1500;

/**
 * A code example in the editor's colours, with a button that copies it.
 * While on screen it is a read-only editor; elsewhere, and until the editor
 * has drawn, the static code holds its place.
 */
export const CodeExample: React.FC<{ code: string }> = ({ code }) => {
    const [copied, setCopied] = useState(false);
    const [live, setLive] = useState(false);
    const containerRef = useRef<HTMLDivElement>(null);
    const near = useNearScreen(containerRef);
    const monaco = useCustomMonaco();

    useEffect(() => {
        if (!copied) {
            return;
        }
        const timer = setTimeout(() => setCopied(false), COPIED_MS);
        return () => clearTimeout(timer);
    }, [copied]);

    const copy = () => {
        electronAPI.clipboard
            .writeText(code)
            .then(() => setCopied(true))
            .catch(console.error);
    };

    return (
        <div
            ref={containerRef}
            className={live ? 'code-example live' : 'code-example'}
        >
            <ColorizedCode code={code} />
            {near && monaco && (
                <ExampleEditor
                    monaco={monaco}
                    code={code}
                    exampleRef={containerRef}
                    onLive={setLive}
                />
            )}
            <button
                type="button"
                className={
                    copied ? 'code-example-copy copied' : 'code-example-copy'
                }
                aria-label={copied ? 'Copied' : 'Copy code'}
                title={copied ? 'Copied' : 'Click to copy'}
                onClick={copy}
            >
                {copied ? <Check size={14} /> : <Copy size={14} />}
            </button>
        </div>
    );
};
