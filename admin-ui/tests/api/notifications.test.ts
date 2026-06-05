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

import { notificationsApi } from '@/api/notifications';

const alert = {
  id: 'a1',
  source: 'amas.sync',
  kind: 'sync_failed',
  severity: 'error',
  title: '同步失败',
  message: '上报数据被软拦截',
  count: 2,
  firstSeenAt: '2026-06-02T01:00:00+00:00',
  lastSeenAt: '2026-06-02T02:00:00+00:00',
  readAt: null,
  ackedBy: null,
};

describe('admin notificationsApi', () => {
  it('list returns inbox items + unreadCount via admin route', async () => {
    server.use(
      http.get(`${BASE}/api/admin/notifications`, ({ request }) => {
        // 默认 list() 不带 unread 过滤
        expect(new URL(request.url).searchParams.get('unread')).toBeNull();
        expect(request.headers.get('Authorization')).toBe('Bearer fake-admin-token');
        return HttpResponse.json({ success: true, data: { items: [alert], unreadCount: 1 } });
      }),
    );
    const result = await notificationsApi.list();
    expect(result.unreadCount).toBe(1);
    expect(result.items).toHaveLength(1);
    expect(result.items[0].id).toBe('a1');
  });

  it('list passes unread=true filter', async () => {
    server.use(
      http.get(`${BASE}/api/admin/notifications`, ({ request }) => {
        expect(new URL(request.url).searchParams.get('unread')).toBe('true');
        return HttpResponse.json({ success: true, data: { items: [], unreadCount: 0 } });
      }),
    );
    const result = await notificationsApi.list(true);
    expect(result.items).toEqual([]);
    expect(result.unreadCount).toBe(0);
  });

  it('markRead posts to admin route and returns updated unreadCount', async () => {
    server.use(
      http.post(`${BASE}/api/admin/notifications/a1/read`, () =>
        HttpResponse.json({ success: true, data: { read: true, unreadCount: 0 } })),
    );
    const result = await notificationsApi.markRead('a1');
    expect(result).toEqual({ read: true, unreadCount: 0 });
  });
});
