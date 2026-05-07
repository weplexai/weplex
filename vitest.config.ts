import { defineConfig } from 'vitest/config';
import { svelte } from '@sveltejs/vite-plugin-svelte';

export default defineConfig({
  plugins: [svelte()],
  resolve: {
    // Test runs against the browser build of Svelte 5; otherwise
    // `mount(...)` from `@testing-library/svelte` resolves to the SSR shim
    // and throws `lifecycle_function_unavailable`.
    conditions: ['browser'],
    extensions: ['.mjs', '.js', '.ts', '.svelte.ts', '.svelte.js', '.svelte'],
  },
  test: {
    environment: 'jsdom',
    globals: false,
    include: ['src/**/*.{test,spec}.{ts,js}'],
  },
});
