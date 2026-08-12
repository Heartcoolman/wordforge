import { defineConfig } from 'vite';
import solid from 'vite-plugin-solid';
import tailwindcss from '@tailwindcss/vite';
import wasm from 'vite-plugin-wasm';
import topLevelAwait from 'vite-plugin-top-level-await';
import { fileURLToPath, URL } from 'node:url';

export default defineConfig({
  // admin-ui 在后端 /admin 子路径托管：资源走 /admin/assets，与根托管的 web-app 资源(/assets)隔离，
  // 避免管理面绝对 /assets 被 web-app fallback 截走导致白屏。SPA 路由本就用 /admin/* 绝对路径，无需改 Router。
  base: '/admin/',
  plugins: [
    solid(),
    tailwindcss(),
    wasm(),
    topLevelAwait(),
    // base='/admin/' 时 dev server 只认 /admin/* —— 两类根路径请求需要补齐与生产一致的行为：
    // ① 裸 /admin → 302 /admin/（否则 404 空白页）；
    // ② 根级 SPA 路由（/login /profile 等 legacy 迁移提示页 + 任意 404 页）→ 改写到
    //    base index.html 交给前端 Router 决定（location.pathname 不变，资产路径 base 绝对不受影响）。
    // e2e 依赖这两条：goto('/admin') 与全部 legacy 路由断言。
    {
      name: 'dev-root-spa-fallback',
      configureServer(server) {
        server.middlewares.use((req, res, next) => {
          const path = (req.url ?? '').split('?')[0];
          if (path === '/admin') {
            res.statusCode = 302;
            res.setHeader('Location', '/admin/');
            res.end();
            return;
          }
          if (
            !path.startsWith('/admin') &&
            !path.startsWith('/api') &&
            !path.startsWith('/health') &&
            !path.startsWith('/@') &&
            !path.includes('.')
          ) {
            req.url = '/admin/index.html';
          }
          next();
        });
      },
    },
  ],
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
