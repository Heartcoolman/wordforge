import { defineConfig } from 'vitest/config';
import solid from 'vite-plugin-solid';
import { fileURLToPath, URL } from 'node:url';

const TEST_API_BASE_URL = 'http://localhost:3000';

export default defineConfig({
  plugins: [solid()],
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url)),
    },
  },
  define: {
    'import.meta.env.VITE_API_BASE_URL': JSON.stringify(TEST_API_BASE_URL),
  },
  test: {
    globals: true,
    environment: 'happy-dom',
    setupFiles: ['./tests/setup.ts'],
    exclude: ['e2e/**', 'node_modules/**'],
    // 用 child_process forks 替代 worker_threads，消除 vitest+v8 coverage+tinypool
    // 在 Linux/Node 20 上 worker 销毁时 libuv 触发的 IPC channel closed 偶发崩溃。
    // 代价：略慢一点，但稳定。
    pool: 'forks',
    env: {
      VITE_API_BASE_URL: TEST_API_BASE_URL,
    },
    coverage: {
      provider: 'v8',
      include: ['src/**/*.{ts,tsx}'],
      exclude: [
        'src/main.tsx',             // bootstrap render entry
        'src/vite-env.d.ts',        // type-only
        'src/types/**',             // type-only
        'src/index.css',            // css
        'src/workers/telemetry.ts', // navigator.sendBeacon worker
        'src/lib/fatigue/**',       // WebRTC + MediaDevices, happy-dom 无原生支持
      ],
      thresholds: {
        lines: 80,
        functions: 80,
        branches: 80,
        statements: 80,
      },
    },
  },
});
