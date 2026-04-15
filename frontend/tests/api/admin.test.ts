import { describe, it, expect, vi, beforeAll, afterAll, afterEach } from 'vitest';
import { setupServer } from 'msw/node';
import { http, HttpResponse } from 'msw';

import { TEST_BASE_URL as BASE } from '../helpers/constants';

const server = setupServer();
beforeAll(() => server.listen({ onUnhandledRequest: 'bypass' }));
afterEach(() => server.resetHandlers());
afterAll(() => server.close());

vi.mock('@/lib/token', () => ({
  tokenManager: {
    getToken: () => null,
    getAdminToken: () => 'fake-admin-token',
    setTokens: vi.fn(),
    clearTokens: vi.fn(),
    needsRefresh: () => false,
    isAuthenticated: () => false,
    setAdminToken: vi.fn(),
  },
}));
vi.mock('@/api/auth', () => ({ authApi: { refresh: vi.fn() } }));

import { adminApi } from '@/api/admin';

describe('adminApi', () => {
  it('checkStatus returns initialized status', async () => {
    server.use(
      http.get(`${BASE}/api/admin/auth/status`, () =>
        HttpResponse.json({ success: true, data: { initialized: true } })),
    );
    const result = await adminApi.checkStatus();
    expect(result).toEqual({ initialized: true });
  });

  it('setup sends email and password', async () => {
    const mockResponse = { token: 'admin-token-123', admin: { id: 'admin-1', email: 'admin@test.com' } };
    server.use(
      http.post(`${BASE}/api/admin/auth/setup`, async ({ request }) => {
        const body = await request.json() as Record<string, unknown>;
        expect(body).toEqual({ email: 'admin@test.com', password: 'secret123' });
        return HttpResponse.json({ success: true, data: mockResponse });
      }),
    );
    const result = await adminApi.setup({ email: 'admin@test.com', password: 'secret123' });
    expect(result).toEqual(mockResponse);
  });

  it('login sends credentials and returns auth response', async () => {
    const mockResponse = { token: 'login-token-456', admin: { id: 'admin-1', email: 'admin@test.com' } };
    server.use(
      http.post(`${BASE}/api/admin/auth/login`, async ({ request }) => {
        const body = await request.json() as Record<string, unknown>;
        expect(body).toEqual({ email: 'admin@test.com', password: 'pass' });
        return HttpResponse.json({ success: true, data: mockResponse });
      }),
    );
    const result = await adminApi.login({ email: 'admin@test.com', password: 'pass' });
    expect(result).toEqual(mockResponse);
  });

  it('logout returns loggedOut status', async () => {
    server.use(
      http.post(`${BASE}/api/admin/auth/logout`, () =>
        HttpResponse.json({ success: true, data: { loggedOut: true } })),
    );
    const result = await adminApi.logout();
    expect(result).toEqual({ loggedOut: true });
  });

  it('getUsers returns list of admin users', async () => {
    const users = {
      data: [{ id: 'u1', email: 'user@test.com', username: 'tester', isBanned: false, failedLoginCount: 0, lockedUntil: null, createdAt: '2026-01-01T00:00:00Z', updatedAt: '2026-01-01T00:00:00Z' }],
      total: 1,
      page: 1,
      perPage: 20,
      totalPages: 1,
    };
    server.use(
      http.get(`${BASE}/api/admin/users`, () =>
        HttpResponse.json({ success: true, data: users })),
    );
    const result = await adminApi.getUsers();
    expect(result).toEqual(users);
  });

  it('banUser sends ban request for specific user', async () => {
    server.use(
      http.post(`${BASE}/api/admin/users/user-42/ban`, () =>
        HttpResponse.json({ success: true, data: { banned: true, userId: 'user-42' } })),
    );
    const result = await adminApi.banUser('user-42');
    expect(result).toEqual({ banned: true, userId: 'user-42' });
  });

  it('unbanUser sends unban request for specific user', async () => {
    server.use(
      http.post(`${BASE}/api/admin/users/user-42/unban`, () =>
        HttpResponse.json({ success: true, data: { banned: false, userId: 'user-42' } })),
    );
    const result = await adminApi.unbanUser('user-42');
    expect(result).toEqual({ banned: false, userId: 'user-42' });
  });

  it('getStats returns admin statistics', async () => {
    const stats = { users: 100, words: 5000, records: 8000, trend: { users: { value: 10, label: '较昨日' } } };
    server.use(
      http.get(`${BASE}/api/admin/stats`, () =>
        HttpResponse.json({ success: true, data: stats })),
    );
    const result = await adminApi.getStats();
    expect(result).toEqual(stats);
  });

  it('getEngagement returns engagement analytics', async () => {
    const engagement = { totalUsers: 100, activeToday: 30, retentionRate: 0.3, trend: { activeToday: { value: 5, label: '较昨日' } } };
    server.use(
      http.get(`${BASE}/api/admin/analytics/engagement`, () =>
        HttpResponse.json({ success: true, data: engagement })),
    );
    const result = await adminApi.getEngagement();
    expect(result).toEqual(engagement);
  });

  it('getLearningAnalytics returns learning analytics', async () => {
    const analytics = { totalWords: 5000, totalRecords: 10000, totalCorrect: 8500, overallAccuracy: 0.85, trend: { totalRecords: { value: 5, label: '较昨日' } } };
    server.use(
      http.get(`${BASE}/api/admin/analytics/learning`, () =>
        HttpResponse.json({ success: true, data: analytics })),
    );
    const result = await adminApi.getLearningAnalytics();
    expect(result).toEqual(analytics);
  });

  it('getHealth returns system health info', async () => {
    const health = { status: 'healthy', dbSizeBytes: 1048576, uptimeSecs: 86400, version: '1.0.0' };
    server.use(
      http.get(`${BASE}/api/admin/monitoring/health`, () =>
        HttpResponse.json({ success: true, data: health })),
    );
    const result = await adminApi.getHealth();
    expect(result).toEqual(health);
  });

  it('getDatabase returns database info', async () => {
    const db = { sizeOnDisk: 123456, tableCount: 15, tables: ['users'], pageSize: 4096, pageCount: 30, walEnabled: true };
    server.use(
      http.get(`${BASE}/api/admin/monitoring/database`, () =>
        HttpResponse.json({ success: true, data: db })),
    );
    const result = await adminApi.getDatabase();
    expect(result).toEqual(db);
  });

  it('broadcast sends notification and returns sent count', async () => {
    server.use(
      http.post(`${BASE}/api/admin/broadcast`, async ({ request }) => {
        const body = await request.json() as Record<string, unknown>;
        expect(body).toEqual({ title: 'Hello', message: 'World' });
        return HttpResponse.json({ success: true, data: { sent: 42 } });
      }),
    );
    const result = await adminApi.broadcast({ title: 'Hello', message: 'World' });
    expect(result).toEqual({ sent: 42 });
  });

  it('getSettings returns system settings', async () => {
    const settings = { maxUsers: 1000, registrationEnabled: true, maintenanceMode: false, defaultDailyWords: 20 };
    server.use(
      http.get(`${BASE}/api/admin/settings`, () =>
        HttpResponse.json({ success: true, data: settings })),
    );
    const result = await adminApi.getSettings();
    expect(result).toEqual(settings);
  });

  it('updateSettings sends partial settings and returns updated settings', async () => {
    const updated = { maxUsers: 1000, registrationEnabled: false, maintenanceMode: false, defaultDailyWords: 20 };
    server.use(
      http.put(`${BASE}/api/admin/settings`, async ({ request }) => {
        const body = await request.json() as Record<string, unknown>;
        expect(body).toEqual({ registrationEnabled: false });
        return HttpResponse.json({ success: true, data: updated });
      }),
    );
    const result = await adminApi.updateSettings({ registrationEnabled: false } as any);
    expect(result).toEqual(updated);
  });

  it('reloadAmas sends config and returns reloaded config', async () => {
    const config = {
      featureFlags: { ensembleEnabled: true, heuristicEnabled: true, igeEnabled: true, swdEnabled: true, mdmEnabled: true },
      ensemble: { baseWeightHeuristic: 0.2, baseWeightIge: 0.4, baseWeightSwd: 0.4, warmupSamples: 20, blendScale: 100, blendMax: 0.5, minWeight: 0.15 },
      modeling: { attentionSmoothing: 0.3, confidenceDecay: 0.99, minConfidence: 0.1, fatigueIncreaseRate: 0.02, fatigueRecoveryRate: 0.001, motivationMomentum: 0.1, visualFatigueWeight: 0.3 },
      constraints: { highFatigueThreshold: 0.9, lowAttentionThreshold: 0.3, lowMotivationThreshold: -0.5, maxBatchSizeWhenFatigued: 5, maxNewRatioWhenFatigued: 0.2, maxDifficultyWhenFatigued: 0.55 },
      monitoring: { sampleRate: 0.05, metricsFlushIntervalSecs: 300 },
      coldStart: { classifyToExploreEvents: 20, classifyToExploreConfidence: 0.6, exploreToExploitEvents: 80 },
      objectiveWeights: { retention: 0.35, accuracy: 0.25, speed: 0.15, fatigue: 0.15, frustration: 0.1 },
    };
    server.use(
      http.post(`${BASE}/api/admin/settings/reload-amas`, async ({ request }) => {
        const body = await request.json() as Record<string, unknown>;
        expect(body).toEqual(config);
        return HttpResponse.json({ success: true, data: config });
      }),
    );
    const result = await adminApi.reloadAmas(config as any);
    expect(result).toEqual(config);
  });
});
