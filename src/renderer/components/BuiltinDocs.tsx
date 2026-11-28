import React from 'react';
import {
    BUILTIN_DRAWINGS,
    BUILTIN_WAVETABLES,
    MI_TABLES_INTRO,
} from '../../shared/builtinAssets';
import { BUILTIN_DX7_VOICES } from '../../shared/builtinDx7';
import { CodeExample } from './CodeExample';
import { useQuoteExample } from '../hooks/useQuoteExample';

/** The Docs panel's Built-ins page: every file bundled with the app. */
export const BuiltinDocs: React.FC = () => {
    const quote = useQuoteExample();
    return (
        <div>
            <h2>Built-in Files</h2>
            <p className="types-intro">
                Files bundled with the app. Load one by its path, which starts
                with <code>builtin/</code>:
            </p>
            <CodeExample
                code={quote(
                    `$wavetable('c3', '${BUILTIN_WAVETABLES[0].path}', 2.5)`,
                )}
            />
            <h3>Mutable Instruments Wavetables</h3>
            <p className="types-intro">{MI_TABLES_INTRO}</p>
            {BUILTIN_WAVETABLES.map((table) => (
                <div key={table.path} className="module-card">
                    <h4>
                        <code>{table.path}</code>
                    </h4>
                    <p>{table.description}</p>
                </div>
            ))}
            <h3>Drawings</h3>
            <p className="types-intro">
                SVG line drawings for <code>$svg</code> to trace.
            </p>
            {BUILTIN_DRAWINGS.map((drawing) => (
                <div key={drawing.path} className="module-card">
                    <h4>
                        <code>{drawing.path}</code>
                    </h4>
                    <p>{drawing.description}</p>
                </div>
            ))}
            <h3>DX7 Voices</h3>
            <p className="types-intro">
                The 96 DX7 patches of Mutable Instruments' Plaits by Emilie
                Gillet, one file per voice, for <code>$mi.sixOp</code>:
            </p>
            <CodeExample
                code={quote(`$mi.sixOp('c3', '${BUILTIN_DX7_VOICES[0].path}')`)}
            />
            {BUILTIN_DX7_VOICES.map((voice) => (
                <div key={voice.path} className="module-card">
                    <h4>
                        <code>{voice.path}</code>
                    </h4>
                    <p>{voice.name}</p>
                </div>
            ))}
        </div>
    );
};
