import mdx from '@mdx-js/rollup';
import react from '@vitejs/plugin-react';
import remarkGfm from 'remark-gfm';
import { defineConfig } from 'vite';

const mdxPlugin = mdx({ remarkPlugins: [remarkGfm] });

// https://vitejs.dev/config
export default defineConfig({
    build: {
        sourcemap: true,
    },
    plugins: [
        // MDX compiles to JSX first, so plugin-react's transform sees it.
        {
            ...mdxPlugin,
            enforce: 'pre',
            // A `?raw` import is the page's source text, left uncompiled.
            // The plugin's own filter reads the path without its query.
            transform(code, id) {
                return /[?&]raw\b/.test(id)
                    ? null
                    : mdxPlugin.transform.call(this, code, id);
            },
        },
        react({
            babel: {
                // By name: plugin-react recognises the compiler only by this
                // string, and turns off `retainLines` for it.
                plugins: ['babel-plugin-react-compiler'],
            },
            include: /\.(mdx|js|jsx|ts|tsx)$/,
        }),
    ],
});
