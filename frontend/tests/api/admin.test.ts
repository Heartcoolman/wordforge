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
    const mockResponse = { token: 'admin-token-123' };
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
    const mockResponse = { token: 'login-token-456' };
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
    const users = [{ id: 'u1', email: 'user@test.com', banned: false }];
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
    const stats = { totalUsers: 100, activeUsers: 50, totalWords: 5000 };
    server.use(
      http.get(`${BASE}/api/admin/stats`, () =>
        HttpResponse.json({ success: true, data: stats })),
    );
    const result = await adminApi.getStats();
    expect(result).toEqual(stats);
  });

  it('getEngagement returns engagement analytics', async () => {
    const engagement = { dailyActiveUsers: 30, weeklyActiveUsers: 80 };
    server.use(
      http.get(`${BASE}/api/admin/analytics/engagement`, () =>
        HttpResponse.json({ success: true, data: engagement })),
    );
    const result = await adminApi.getEngagement();
    expect(result).toEqual(engagement);
  });

  it('getLearningAnalytics returns learning analytics', async () => {
    const analytics = { avgAccuracy: 0.85, avgSessionTime: 120 };
    server.use(
      http.get(`${BASE}/api/admin/analytics/learning`, () =>
        HttpResponse.json({ success: true, data: analytics })),
    );
    const result = await adminApi.getLearningAnalytics();
    expect(result).toEqual(analytics);
  });

  it('getHealth returns system health info', async () => {
    const health = { status: 'healthy', uptime: 86400 };
    server.use(
      http.get(`${BASE}/api/admin/monitoring/health`, () =>
        HttpResponse.json({ success: true, data: health })),
    );
    const result = await adminApi.getHealth();
    expect(result).toEqual(health);
  });

  it('getDatabase returns database info', async () => {
    const db = { size: '1.2GB', tables: 15 };
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
    const settings = { registrationOpen: true, maxUsersPerDay: 100 };
    server.use(
      http.get(`${BASE}/api/admin/settings`, () =>
        HttpResponse.json({ success: true, data: settings })),
    );
    const result = await adminApi.getSettings();
    expect(result).toEqual(settings);
  });

  it('updateSettings sends partial settings and returns updated settings', async () => {
    const updated = { registrationOpen: false, maxUsersPerDay: 100 };
    server.use(
      http.put(`${BASE}/api/admin/settings`, async ({ request }) => {
        const body = await request.json() as Record<string, unknown>;
        expect(body).toEqual({ registrationOpen: false });
        return HttpResponse.json({ success: true, data: updated });
      }),
    );
    const result = await adminApi.updateSettings({ registrationOpen: false } as any);
    expect(result).toEqual(updated);
  });
});
