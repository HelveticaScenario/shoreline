import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

// https://vitejs.dev/config
export default defineConfig({
    build: {
        sourcemap: true,
    },
    plugins: [
        react({
            babel: {
                // By name: plugin-react recognises the compiler only by this
                // string, and turns off `retainLines` for it.
                plugins: ['babel-plugin-react-compiler'],
            },
        }),
    ],
});
