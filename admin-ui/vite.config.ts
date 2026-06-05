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
        // v1.1.3-E4：函数式 manualChunks。echarts/codemirror 用子路径导入
        // （echarts/core、@codemirror/*），对象式 ['echarts'] 匹配不到子模块，
        // 改 id.includes 命中整个包目录 + 传递依赖（zrender / @lezer / crelt …），
        // 确保跨路由去重 + 长期缓存稳定（价值不在减首屏——首屏已被路由级 lazy 隔离）。
        manualChunks(id) {
          if (!id.includes('node_modules')) return;
          if (id.includes('/echarts/') || id.includes('/zrender/')) return 'vendor-echarts';
          if (
            id.includes('/@codemirror/') ||
            id.includes('/@lezer/') ||
            id.includes('/codemirror/') ||
            id.includes('/crelt/') ||
            id.includes('/style-mod/') ||
            id.includes('/w3c-keyname/')
          )
            return 'vendor-codemirror';
          if (id.includes('/solid-js/')) return 'vendor-solid';
          if (id.includes('/@solidjs/router/')) return 'vendor-router';
          if (id.includes('/@mediapipe/')) return 'vendor-mediapipe';
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
