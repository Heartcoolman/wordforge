import { describe, it, expect, vi, beforeAll, afterAll, afterEach } from 'vitest';
import { setupServer } from 'msw/node';
import { http, HttpResponse } from 'msw';

import { TEST_BASE_URL as BASE } from '../helpers/constants';

vi.mock('@/lib/token', () => ({
  tokenManager: {
    getToken: () => null,
    getAdminToken: () => 'admin-tok',
    setTokens: vi.fn(),
    clearTokens: vi.fn(),
    clearAdminToken: vi.fn(),
    needsRefresh: () => false,
    refreshAccessToken: vi.fn(),
    isAuthenticated: () => false,
    setAdminToken: vi.fn(),
  },
}));

import { statusApi } from '@/api/status';
import type { AppStatus } from '@/api/status';

const server = setupServer();
beforeAll(() => server.listen({ onUnhandledRequest: 'bypass' }));
afterEach(() => server.resetHandlers());
afterAll(() => server.close());

describe('statusApi', () => {
  it('get() 打 GET /api/status 并返回解包后的 AppStatus', async () => {
    const status: AppStatus = {
      maintenanceMode: false,
      version: '0.6.1',
      webTargetVersion: '1.2.3',
      webPwaSilentUpdate: true,
      forceUpgrade: false,
      latestVersion: '0.6.2',
    };
    let method = '';
    let url = '';
    server.use(
      http.get(`${BASE}/api/status`, ({ request }) => {
        method = request.method;
        url = request.url;
        return HttpResponse.json({ success: true, data: status });
      }),
    );
    const res = await statusApi.get();
    expect(method).toBe('GET');
    expect(url).toBe(`${BASE}/api/status`);
    expect(res).toEqual(status);
  });

  it('维护模式 / 强升 / 未发布 web 构建的字段组合原样透传', async () => {
    const status: AppStatus = {
      maintenanceMode: true,
      version: null,
      webTargetVersion: null,
      webPwaSilentUpdate: false,
      forceUpgrade: true,
      latestVersion: null,
    };
    server.use(http.get(`${BASE}/api/status`, () => HttpResponse.json({ success: true, data: status })));
    const res = await statusApi.get();
    expect(res.maintenanceMode).toBe(true);
    expect(res.forceUpgrade).toBe(true);
    expect(res.webTargetVersion).toBeNull();
  });

  it('后端返回错误包时 reject', async () => {
    server.use(
      http.get(`${BASE}/api/status`, () =>
        HttpResponse.json({ success: false, error: { message: 'status boom' } }, { status: 500 })),
    );
    await expect(statusApi.get()).rejects.toThrow();
  });
});
