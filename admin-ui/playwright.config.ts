import { defineConfig, devices } from '@playwright/test';

// 端口可经 E2E_PORT 覆盖（默认 5173）。本地若 5173 被其它项目（如 wordforge-web）的 dev
// server 占用，用 E2E_PORT=<空闲端口> 即可在干净端口起 admin-ui 自己的 vite，避免复用到错误的 app。
// CI 不设 E2E_PORT，沿用 5173 且 reuseExistingServer=false（每次全新启动）。
const PORT = Number(process.env.E2E_PORT ?? 5173);
const BASE_URL = `http://localhost:${PORT}`;

export default defineConfig({
  testDir: './e2e',
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: process.env.CI ? 1 : undefined,
  reporter: 'html',
  use: {
    baseURL: BASE_URL,
    trace: 'on-first-retry',
  },
  projects: [
    { name: 'chromium', use: { ...devices['Desktop Chrome'] } },
  ],
  webServer: {
    command: `npm run dev -- --port ${PORT} --strictPort`,
    url: BASE_URL,
    reuseExistingServer: !process.env.CI,
  },
});
