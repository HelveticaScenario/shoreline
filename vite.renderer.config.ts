import mdx from '@mdx-js/rollup';
import react from '@vitejs/plugin-react';
import remarkGfm from 'remark-gfm';
import { defineConfig } from 'vite';

// https://vitejs.dev/config
export default defineConfig({
    build: {
        sourcemap: true,
    },
    plugins: [
        // MDX compiles to JSX first, so plugin-react's transform sees it.
        { enforce: 'pre', ...mdx({ remarkPlugins: [remarkGfm] }) },
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
