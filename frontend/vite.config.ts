import { defineConfig } from 'vite';
import solid from 'vite-plugin-solid';
import tailwindcss from '@tailwindcss/vite';
import wasm from 'vite-plugin-wasm';
import topLevelAwait from 'vite-plugin-top-level-await';
import { fileURLToPath, URL } from 'node:url';

export default defineConfig({
  plugins: [solid(), tailwindcss(), wasm(), topLevelAwait()],
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url)),
      '@fatigue-wasm': fileURLToPath(new URL('../crates/visual-fatigue-wasm/pkg', import.meta.url)),
    },
  },
  server: {
    port: 5173,
    proxy: {
      '/api': 'http://localhost:3000',
      '/health': 'http://localhost:3000',
    },
  },
  build: {
    outDir: '../static',
    emptyOutDir: true,
    target: 'esnext', // 确保支持 top-level await
    sourcemap: 'hidden',
    rollupOptions: {
      output: {
        manualChunks: {
          'vendor-solid': ['solid-js', 'solid-js/web', 'solid-js/store'],
          'vendor-router': ['@solidjs/router'],
          'vendor-query': ['@tanstack/solid-query'],
          'vendor-mediapipe': ['@mediapipe/tasks-vision'],
        },
      },
    },
  },
  esbuild: {
    drop: ['debugger'],
    pure: ['console.log', 'console.debug', 'console.info'],
  },
  worker: {
    format: 'es',
    plugins: () => [wasm()],
  },
});
