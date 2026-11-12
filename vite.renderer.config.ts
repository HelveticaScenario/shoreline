import react from '@vitejs/plugin-react';
import reactCompiler from 'babel-plugin-react-compiler';
import { defineConfig } from 'vite';

// https://vitejs.dev/config
export default defineConfig({
    build: {
        sourcemap: true,
    },
    plugins: [
        react({
            babel: {
                plugins: [reactCompiler],
            },
        }),
    ],
});
