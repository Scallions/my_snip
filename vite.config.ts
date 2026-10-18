import { defineConfig } from 'vitest/config';
import { svelte } from '@sveltejs/vite-plugin-svelte';

export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: {
    port: 1420, strictPort: true, host: '127.0.0.1',
    watch: { ignored: ['**/src-tauri/**', '**/test-results/**', '**/output/**', '**/.cache/**'] },
  },
  envPrefix: ['VITE_'],
  build: { target: ['es2022', 'chrome105', 'safari15'], sourcemap: false },
  test: { include: ['src/**/*.test.ts'] },
});
