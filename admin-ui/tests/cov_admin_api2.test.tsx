import { describe, it, expect, vi, beforeAll, afterAll, afterEach } from 'vitest';
import { setupServer } from 'msw/node';
import { http, HttpResponse } from 'msw';

import { TEST_BASE_URL as BASE } from './helpers/constants';

// 本文件补 tests/api/admin.test.ts 未覆盖的导出函数与分支：
//  - updatesHistory（admin.ts 唯一未覆盖行 131）
//  - setMaintenance true/false body 契约
//  - 非 2xx 抛 ApiError（unwrap 错误分支）
//  - amasListSuggestions 不传 status 时 URL 省略 status
//  - getTelemetry / wbCenterPreview 不传 params 分支
// 全程 MSW 拦截，绝不真连网络。

const server = setupServer();
beforeAll(() => server.listen({ onUnhandledRequest: 'error' }));
afterEach(() => server.resetHandlers());
afterAll(() => server.close());

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
    refreshAccessToken: vi.fn(async () => false),
  },
}));
vi.mock('@/api/auth', () => ({ authApi: { refresh: vi.fn() } }));

import { adminApi } from '@/api/admin';
import { ApiError } from '@/api/http';

describe('adminApi —— 补充覆盖未测函数/分支', () => {
  it('updatesHistory 命中 GET /updates/history 并解析 entries', async () => {
    const entries = [
      {
        id: 1,
        fromVersion: 'v1.1.3',
        toVersion: 'v1.1.4',
        channel: 'beta',
        status: 'success',
        startedAt: '2026-06-04T10:00:00Z',
        finishedAt: '2026-06-04T10:00:30Z',
      },
    ];
    let method = '';
    server.use(
      http.get(`${BASE}/api/admin/updates/history`, ({ request }) => {
        method = request.method;
        return HttpResponse.json({ success: true, data: { entries } });
      }),
    );
    const result = await adminApi.updatesHistory();
    expect(method).toBe('GET');
    expect(result).toEqual({ entries });
    expect(result.entries).toHaveLength(1);
  });

  it('updatesHistory 空列表正常解析', async () => {
    server.use(
      http.get(`${BASE}/api/admin/updates/history`, () =>
        HttpResponse.json({ success: true, data: { entries: [] } })),
    );
    const result = await adminApi.updatesHistory();
    expect(result.entries).toEqual([]);
  });

  it('setMaintenance(true) POST body { active: true }', async () => {
    let body: Record<string, unknown> = {};
    server.use(
      http.post(`${BASE}/api/admin/settings/maintenance`, async ({ request }) => {
        body = (await request.json()) as Record<string, unknown>;
        return HttpResponse.json({ success: true, data: { active: true } });
      }),
    );
    const result = await adminApi.setMaintenance(true);
    expect(body).toEqual({ active: true });
    expect(result).toEqual({ active: true });
  });

  it('setMaintenance(false) POST body { active: false }', async () => {
    let body: Record<string, unknown> = {};
    server.use(
      http.post(`${BASE}/api/admin/settings/maintenance`, async ({ request }) => {
        body = (await request.json()) as Record<string, unknown>;
        return HttpResponse.json({ success: true, data: { active: false } });
      }),
    );
    const result = await adminApi.setMaintenance(false);
    expect(body).toEqual({ active: false });
    expect(result).toEqual({ active: false });
  });

  it('amasListSuggestions 不传 status 时 URL 省略 status，仅 limit', async () => {
    let hasStatus = true;
    let limit: string | null = null;
    server.use(
      http.get(`${BASE}/api/admin/amas/suggestions`, ({ request }) => {
        const url = new URL(request.url);
        hasStatus = url.searchParams.has('status');
        limit = url.searchParams.get('limit');
        return HttpResponse.json({ success: true, data: [] });
      }),
    );
    const result = await adminApi.amasListSuggestions();
    expect(hasStatus).toBe(false);
    expect(limit).toBe('50');
    expect(result).toEqual([]);
  });

  it('getTelemetry 不传 params 时 URL 无 limit/offset', async () => {
    let hadLimit = true;
    let hadOffset = true;
    server.use(
      http.get(`${BASE}/api/admin/telemetry/dev-x`, ({ request }) => {
        const url = new URL(request.url);
        hadLimit = url.searchParams.has('limit');
        hadOffset = url.searchParams.has('offset');
        return HttpResponse.json({ success: true, data: { records: [], total: 0 } });
      }),
    );
    const result = await adminApi.getTelemetry('dev-x');
    expect(hadLimit).toBe(false);
    expect(hadOffset).toBe(false);
    expect(result).toEqual({ records: [], total: 0 });
  });

  it('wbCenterPreview 不传 params 分支', async () => {
    let hadPage = true;
    server.use(
      http.get(`${BASE}/api/admin/wordbook-center/browse/wb-z`, ({ request }) => {
        hadPage = new URL(request.url).searchParams.has('page');
        return HttpResponse.json({ success: true, data: { id: 'wb-z', title: 't', words: [], total: 0 } });
      }),
    );
    const result = await adminApi.wbCenterPreview('wb-z');
    expect(hadPage).toBe(false);
    expect(result).toMatchObject({ id: 'wb-z' });
  });

  it('getDatabase 命中 GET /monitoring/database', async () => {
    let method = '';
    server.use(
      http.get(`${BASE}/api/admin/monitoring/database`, ({ request }) => {
        method = request.method;
        return HttpResponse.json({ success: true, data: { sizeOnDisk: 1, tableCount: 1, tables: [], pageSize: 4096, pageCount: 1, walEnabled: true } });
      }),
    );
    const result = await adminApi.getDatabase();
    expect(method).toBe('GET');
    expect(result.tableCount).toBe(1);
  });

  // ── 非 2xx 抛 ApiError（unwrap 错误分支）──
  it('非 2xx（500）抛 ApiError 并带 code/message', async () => {
    server.use(
      http.get(`${BASE}/api/admin/updates/history`, () =>
        HttpResponse.json({ code: 'INTERNAL', message: '服务器内部错误' }, { status: 500 })),
    );
    await expect(adminApi.updatesHistory()).rejects.toBeInstanceOf(ApiError);
    try {
      await adminApi.updatesHistory();
    } catch (e) {
      const err = e as ApiError;
      expect(err.status).toBe(500);
      expect(err.code).toBe('INTERNAL');
      expect(err.message).toBe('服务器内部错误');
    }
  });

  it('success:false 信封抛 ApiError(API_ERROR)', async () => {
    server.use(
      http.get(`${BASE}/api/admin/stats`, () =>
        HttpResponse.json({ success: false, code: 'FORBIDDEN', message: '无权限' })),
    );
    try {
      await adminApi.getStats();
      throw new Error('应当抛错');
    } catch (e) {
      expect(e).toBeInstanceOf(ApiError);
      const err = e as ApiError;
      expect(err.code).toBe('FORBIDDEN');
      expect(err.message).toBe('无权限');
    }
  });

  it('404（无 code 字段）回退到 UNKNOWN', async () => {
    server.use(
      http.get(`${BASE}/api/admin/settings`, () =>
        HttpResponse.json({}, { status: 404 })),
    );
    try {
      await adminApi.getSettings();
      throw new Error('应当抛错');
    } catch (e) {
      const err = e as ApiError;
      expect(err).toBeInstanceOf(ApiError);
      expect(err.status).toBe(404);
      expect(err.code).toBe('UNKNOWN');
    }
  });
});