import { describe, it, expect, vi, beforeAll, afterAll, afterEach } from 'vitest';
import { setupServer } from 'msw/node';
import { http, HttpResponse } from 'msw';

import { TEST_BASE_URL as BASE } from '../helpers/constants';

vi.mock('@/lib/token', () => ({
  tokenManager: {
    getToken: () => null,
    getAdminToken: () => null,
    setTokens: vi.fn(),
    clearTokens: vi.fn(),
    needsRefresh: () => false,
    isAuthenticated: () => false,
    setAdminToken: vi.fn(),
  },
}));
vi.mock('@/api/auth', () => ({ authApi: { refresh: vi.fn() } }));

import { usersApi } from '@/api/users';

const server = setupServer();
beforeAll(() => server.listen({ onUnhandledRequest: 'bypass' }));
afterEach(() => server.resetHandlers());
afterAll(() => server.close());

describe('usersApi', () => {
  it('getMe fetches current user', async () => {
    const user = { id: 'u1', email: 'test@test.com', username: 'tester', isBanned: false };
    server.use(
      http.get(`${BASE}/api/users/me`, () =>
        HttpResponse.json({ success: true, data: user })),
    );
    const result = await usersApi.getMe();
    expect(result).toEqual(user);
  });

  it('updateMe sends PUT with username', async () => {
    const updated = { id: 'u1', email: 'test@test.com', username: 'newname', isBanned: false };
    server.use(
      http.put(`${BASE}/api/users/me`, () =>
        HttpResponse.json({ success: true, data: updated })),
    );
    const result = await usersApi.updateMe({ username: 'newname' });
    expect(result).toEqual(updated);
  });

  it('changePassword sends PUT to password endpoint', async () => {
    server.use(
      http.put(`${BASE}/api/users/me/password`, () =>
        HttpResponse.json({ success: true, data: { passwordChanged: true } })),
    );
    const result = await usersApi.changePassword({ currentPassword: 'old', newPassword: 'newpw123' });
    expect(result).toEqual({ passwordChanged: true });
  });

  it('getStats fetches user statistics', async () => {
    const stats = { totalWordsLearned: 150, totalSessions: 30, totalRecords: 500, streakDays: 7, accuracyRate: 0.85 };
    server.use(
      http.get(`${BASE}/api/users/me/stats`, () =>
        HttpResponse.json({ success: true, data: stats })),
    );
    const result = await usersApi.getStats();
    expect(result).toEqual(stats);
  });
});
