import { describe, it, expect, vi, beforeAll, afterAll, afterEach } from 'vitest';
import { setupServer } from 'msw/node';
import { http, HttpResponse } from 'msw';

const BASE = 'http://localhost:3000';

vi.mock('@/lib/token', () => ({
  tokenManager: {
    getToken: () => null,
    getAdminToken: () => null,
    setTokens: vi.fn(),
    clearTokens: vi.fn(),
    needsRefresh: () => false,
    isAuthenticated: () => false,
    setAdminToken: vi.fn(),
    getRefreshToken: () => 'fake-refresh',
  },
}));

import { authApi } from '@/api/auth';

const server = setupServer();
beforeAll(() => server.listen({ onUnhandledRequest: 'bypass' }));
afterEach(() => server.resetHandlers());
afterAll(() => server.close());

describe('authApi', () => {
  it('login sends credentials and returns auth response', async () => {
    const authRes = { accessToken: 'tok', refreshToken: 'ref', user: { id: '1', email: 'a@b.com', username: 'test', isBanned: false } };
    server.use(
      http.post(`${BASE}/api/auth/login`, () =>
        HttpResponse.json({ success: true, data: authRes })),
    );
    const result = await authApi.login({ email: 'a@b.com', password: 'pass' });
    expect(result).toEqual(authRes);
  });

  it('register sends registration data', async () => {
    const authRes = { accessToken: 'tok', refreshToken: 'ref', user: { id: '2', email: 'b@c.com', username: 'user2', isBanned: false } };
    server.use(
      http.post(`${BASE}/api/auth/register`, () =>
        HttpResponse.json({ success: true, data: authRes })),
    );
    const result = await authApi.register({ email: 'b@c.com', username: 'user2', password: 'pass' });
    expect(result).toEqual(authRes);
  });

  it('refresh returns new tokens', async () => {
    const data = { accessToken: 'new-tok', refreshToken: 'new-ref', user: { id: '1', email: 'a@b.com', username: 'test', isBanned: false } };
    server.use(
      http.post(`${BASE}/api/auth/refresh`, () =>
        HttpResponse.json({ success: true, data })),
    );
    const result = await authApi.refresh();
    expect(result).toEqual(data);
  });

  it('logout sends logout request', async () => {
    server.use(
      http.post(`${BASE}/api/auth/logout`, () =>
        HttpResponse.json({ success: true, data: { loggedOut: true } })),
    );
    const result = await authApi.logout();
    expect(result).toEqual({ loggedOut: true });
  });

  it('forgotPassword sends email', async () => {
    server.use(
      http.post(`${BASE}/api/auth/forgot-password`, () =>
        HttpResponse.json({ success: true, data: { success: true } })),
    );
    const result = await authApi.forgotPassword('user@test.com');
    expect(result).toEqual({ success: true });
  });

  it('resetPassword sends token and new password', async () => {
    server.use(
      http.post(`${BASE}/api/auth/reset-password`, () =>
        HttpResponse.json({ success: true, data: { success: true } })),
    );
    const result = await authApi.resetPassword('reset-token-123', 'newpass123');
    expect(result).toEqual({ success: true });
  });
});
