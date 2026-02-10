import { describe, it, expect, vi, beforeAll, afterAll, afterEach } from 'vitest';
import { setupServer } from 'msw/node';
import { http, HttpResponse } from 'msw';

const BASE = 'http://localhost:3000';

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

import { notificationsApi } from '@/api/notifications';

describe('notificationsApi', () => {
  it('list returns notifications', async () => {
    const notifications = [
      { id: 'n1', title: 'Welcome', message: 'Hello!', read: false, createdAt: '2026-02-10' },
    ];
    server.use(
      http.get(`${BASE}/api/notifications`, () =>
        HttpResponse.json({ success: true, data: notifications })),
    );
    const result = await notificationsApi.list();
    expect(result).toEqual(notifications);
  });

  it('list sends query params when provided', async () => {
    server.use(
      http.get(`${BASE}/api/notifications`, ({ request }) => {
        const url = new URL(request.url);
        expect(url.searchParams.get('limit')).toBe('10');
        expect(url.searchParams.get('unreadOnly')).toBe('true');
        return HttpResponse.json({ success: true, data: [] });
      }),
    );
    const result = await notificationsApi.list({ limit: 10, unreadOnly: true });
    expect(result).toEqual([]);
  });

  it('markRead marks a notification as read', async () => {
    server.use(
      http.put(`${BASE}/api/notifications/n1/read`, () =>
        HttpResponse.json({ success: true, data: { read: true } })),
    );
    const result = await notificationsApi.markRead('n1');
    expect(result).toEqual({ read: true });
  });

  it('markAllRead marks all notifications as read', async () => {
    server.use(
      http.post(`${BASE}/api/notifications/read-all`, () =>
        HttpResponse.json({ success: true, data: { markedRead: 5 } })),
    );
    const result = await notificationsApi.markAllRead();
    expect(result).toEqual({ markedRead: 5 });
  });

  it('getBadges returns list of badges', async () => {
    const badges = [{ id: 'b1', name: 'First Word', icon: 'star' }];
    server.use(
      http.get(`${BASE}/api/notifications/badges`, () =>
        HttpResponse.json({ success: true, data: badges })),
    );
    const result = await notificationsApi.getBadges();
    expect(result).toEqual(badges);
  });

  it('getPreferences returns user preferences', async () => {
    const prefs = { emailNotifications: true, pushNotifications: false };
    server.use(
      http.get(`${BASE}/api/notifications/preferences`, () =>
        HttpResponse.json({ success: true, data: prefs })),
    );
    const result = await notificationsApi.getPreferences();
    expect(result).toEqual(prefs);
  });

  it('updatePreferences sends partial data and returns updated preferences', async () => {
    const updated = { emailNotifications: false, pushNotifications: false };
    server.use(
      http.put(`${BASE}/api/notifications/preferences`, async ({ request }) => {
        const body = await request.json() as Record<string, unknown>;
        expect(body).toEqual({ emailNotifications: false });
        return HttpResponse.json({ success: true, data: updated });
      }),
    );
    const result = await notificationsApi.updatePreferences({ emailNotifications: false } as any);
    expect(result).toEqual(updated);
  });
});
