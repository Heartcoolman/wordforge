import { describe, it, expect, vi, beforeAll, afterAll, afterEach } from 'vitest';
import { setupServer } from 'msw/node';
import { http, HttpResponse } from 'msw';

import { TEST_BASE_URL as BASE } from '../helpers/constants';

vi.mock('@/lib/token', () => ({
  tokenManager: {
    getToken: () => null,
    getAdminToken: () => 'fake-admin-token',
    setTokens: vi.fn(),
    clearTokens: vi.fn(),
    clearAdminToken: vi.fn(),
    needsRefresh: () => false,
    isAuthenticated: () => false,
    setAdminToken: vi.fn(),
    refreshAccessToken: vi.fn().mockResolvedValue(false),
  },
}));
vi.mock('@/api/auth', () => ({ authApi: { refresh: vi.fn() } }));

import { healthApi } from '@/api/health';

const server = setupServer();
beforeAll(() => server.listen({ onUnhandledRequest: 'bypass' }));
afterEach(() => server.resetHandlers());
afterAll(() => server.close());

describe('healthApi', () => {
  it('getStatus returns public health summary', async () => {
    const status = {
      status: 'healthy',
      uptimeSecs: 100,
      services: {
        store: { healthy: true },
        amas: { healthy: true },
        sse: { healthy: true },
        wordbookCenter: { healthy: true, url: 'https://wbc.example' },
      },
    };
    server.use(
      http.get(`${BASE}/health`, () =>
        HttpResponse.json({ success: true, data: status })),
    );
    const result = await healthApi.getStatus();
    expect(result).toEqual(status);
  });

  it('getLiveness resolves with { live: true } on 204', async () => {
    server.use(
      http.get(`${BASE}/health/live`, () => new HttpResponse(null, { status: 204 })),
    );
    const result = await healthApi.getLiveness();
    expect(result).toEqual({ live: true });
  });

  it('getReadiness resolves with { ready: true } on 204', async () => {
    server.use(
      http.get(`${BASE}/health/ready`, () => new HttpResponse(null, { status: 204 })),
    );
    const result = await healthApi.getReadiness();
    expect(result).toEqual({ ready: true });
  });

  it('getDatabase returns latency snapshot via admin token', async () => {
    const db = { healthy: true, latencyUs: 1234, consecutiveFailures: 0 };
    let authHeader = '';
    server.use(
      http.get(`${BASE}/health/database`, ({ request }) => {
        authHeader = request.headers.get('authorization') ?? '';
        return HttpResponse.json({ success: true, data: db });
      }),
    );
    const result = await healthApi.getDatabase();
    expect(result).toEqual(db);
    expect(authHeader).toBe('Bearer fake-admin-token');
  });

  it('getMetrics returns algorithm metrics envelope', async () => {
    const metrics = { algorithms: { heuristic: { callCount: 1, totalLatencyUs: 10, errorCount: 0 } } };
    server.use(
      http.get(`${BASE}/health/metrics`, () =>
        HttpResponse.json({ success: true, data: metrics })),
    );
    const result = await healthApi.getMetrics();
    expect(result).toEqual(metrics);
  });
});
