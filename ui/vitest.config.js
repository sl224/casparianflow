import { defineConfig } from 'vitest/config';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import path from 'path';

export default defineConfig({
  plugins: [svelte({ hot: !process.env.VITEST })],
  test: {
    include: ['src/**/*.{test,spec}.{js,ts}'],
    environment: 'happy-dom',
    globals: true,
    setupFiles: ['./vitest.setup.js'],
    alias: {
      '$lib': path.resolve(__dirname, './src/lib'),
      '$app': path.resolve(__dirname, './node_modules/@sveltejs/kit/src/runtime/app'),
    },
  },
});
