import React, { useEffect, useState } from 'react';
import { Check, Copy } from 'lucide-react';
import electronAPI from '../electronAPI';
import { ColorizedCode } from './ColorizedCode';
import './CodeExample.css';

/** How long the button shows that the copy happened. */
const COPIED_MS = 1500;

/** A code example in the editor's colours, with a button that copies it. */
export const CodeExample: React.FC<{ code: string }> = ({ code }) => {
    const [copied, setCopied] = useState(false);

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
        <div className="code-example">
            <ColorizedCode code={code} />
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
