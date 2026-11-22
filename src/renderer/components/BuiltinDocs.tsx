import React from 'react';
import {
    BUILTIN_WAVETABLES,
    MI_TABLES_INTRO,
} from '../../shared/builtinAssets';
import { ColorizedCode } from './ColorizedCode';
import { useQuoteExample } from '../hooks/useQuoteStyle';

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
            <ColorizedCode
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
        </div>
    );
};
