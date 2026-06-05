import { describe, it, expect, vi, beforeAll, afterAll, afterEach } from 'vitest';
import { setupServer } from 'msw/node';
import { http, HttpResponse } from 'msw';

import { TEST_BASE_URL as BASE } from './helpers/constants';

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

describe('adminApi - 补充覆盖（cov_admin_api）', () => {
  // ─────────── setMaintenance（line 137 未覆盖） ───────────
  it('setMaintenance 发送 active:true 并解析返回', async () => {
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

  it('setMaintenance 发送 active:false', async () => {
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

  // ─────────── updatesHistory（line 131 未覆盖） ───────────
  it('updatesHistory 命中 history 端点并返回审计条目', async () => {
    const entries = [
      { id: 1, fromVersion: 'v1.1.0', toVersion: 'v1.1.1', channel: 'stable', status: 'success', startedAt: '2026-06-01T00:00:00Z' },
    ];
    server.use(
      http.get(`${BASE}/api/admin/updates/history`, () =>
        HttpResponse.json({ success: true, data: { entries } })),
    );
    const result = await adminApi.updatesHistory();
    expect(result.entries).toEqual(entries);
  });

  it('updatesHistory 空列表不抛错', async () => {
    server.use(
      http.get(`${BASE}/api/admin/updates/history`, () =>
        HttpResponse.json({ success: true, data: { entries: [] } })),
    );
    const result = await adminApi.updatesHistory();
    expect(Array.isArray(result.entries)).toBe(true);
    expect(result.entries).toHaveLength(0);
  });

  // ─────────── 错误分支：非 2xx 抛 ApiError（经 admin.ts 透传） ───────────
  it('getStats 命中 500 时抛 ApiError，携带后端 code/message', async () => {
    server.use(
      http.get(`${BASE}/api/admin/stats`, () =>
        HttpResponse.json(
          { code: 'INTERNAL', message: '内部错误' },
          { status: 500 },
        )),
    );
    await expect(adminApi.getStats()).rejects.toMatchObject({
      name: 'ApiError',
      status: 500,
      code: 'INTERNAL',
      message: '内部错误',
    });
  });

  it('getSettings 命中 403 抛 ApiError（无 body 时 fallback 到 statusText 语义）', async () => {
    server.use(
      http.get(`${BASE}/api/admin/settings`, () =>
        new HttpResponse(null, { status: 403 })),
    );
    const err = await adminApi.getSettings().catch((e) => e);
    expect(err).toBeInstanceOf(ApiError);
    expect((err as ApiError).status).toBe(403);
    expect((err as ApiError).code).toBe('UNKNOWN');
  });

  it('banUser 命中 404 抛 ApiError', async () => {
    server.use(
      http.post(`${BASE}/api/admin/users/missing/ban`, () =>
        HttpResponse.json({ code: 'NOT_FOUND', message: '用户不存在' }, { status: 404 })),
    );
    await expect(adminApi.banUser('missing')).rejects.toMatchObject({
      status: 404,
      code: 'NOT_FOUND',
    });
  });

  it('updatesApply 命中 409 抛 ApiError（版本冲突场景）', async () => {
    server.use(
      http.post(`${BASE}/api/admin/updates/apply`, () =>
        HttpResponse.json({ code: 'VERSION_MISMATCH', message: '当前版本不匹配' }, { status: 409 })),
    );
    await expect(
      adminApi.updatesApply('stable', 'v1.2.0', 'v1.1.0'),
    ).rejects.toMatchObject({ status: 409, code: 'VERSION_MISMATCH' });
  });

  // ─────────── success:false 包裹体 → API_ERROR 分支 ───────────
  it('checkStatus 返回 success:false 时抛 API_ERROR', async () => {
    server.use(
      http.get(`${BASE}/api/admin/auth/status`, () =>
        HttpResponse.json({ success: false, code: 'BAD', message: '失败了' })),
    );
    await expect(adminApi.checkStatus()).rejects.toMatchObject({
      code: 'BAD',
      message: '失败了',
    });
  });

  // ─────────── 429 限流分支：带 Retry-After ───────────
  it('getHealth 命中 429 抛 RATE_LIMITED 并携带 retryAfter', async () => {
    server.use(
      http.get(`${BASE}/api/admin/monitoring/health`, () =>
        HttpResponse.json(
          { code: 'RATE_LIMITED', message: 'slow down' },
          { status: 429, headers: { 'Retry-After': '12' } },
        )),
    );
    const err = await adminApi.getHealth().catch((e) => e);
    expect(err).toBeInstanceOf(ApiError);
    expect((err as ApiError).status).toBe(429);
    expect((err as ApiError).retryAfter).toBe(12);
    expect((err as ApiError).message).toContain('12');
  });

  // ─────────── paginated data.data 解析（listFeedback 契约复测，含查询透传） ───────────
  it('listFeedback 透传分页参数且 data 字段为列表', async () => {
    const item = {
      id: 'fb-9', userId: 'u-9', category: 'bug', body: 'x', route: '/r',
      createdAt: '2026-06-01T00:00:00Z',
    };
    let q: URLSearchParams | null = null;
    server.use(
      http.get(`${BASE}/api/admin/feedback`, ({ request }) => {
        q = new URL(request.url).searchParams;
        return HttpResponse.json({
          success: true,
          data: { data: [item], total: 1, page: 2, perPage: 10, totalPages: 1 },
        });
      }),
    );
    const result = await adminApi.listFeedback({ page: 2, perPage: 10 });
    expect(q!.get('page')).toBe('2');
    expect(q!.get('perPage')).toBe('10');
    expect(result.data).toEqual([item]);
    expect(result.page).toBe(2);
    expect(result.perPage).toBe(10);
  });

  it('listFeedback 不传参数时不附加 query', async () => {
    let hadPage = true;
    server.use(
      http.get(`${BASE}/api/admin/feedback`, ({ request }) => {
        hadPage = new URL(request.url).searchParams.has('page');
        return HttpResponse.json({
          success: true,
          data: { data: [], total: 0, page: 1, perPage: 20, totalPages: 0 },
        });
      }),
    );
    await adminApi.listFeedback();
    expect(hadPage).toBe(false);
  });

  // ─────────── getTelemetry：不传 params 时不附加 query ───────────
  it('getTelemetry 不传 params 时不附加 limit/offset', async () => {
    let hadLimit = true;
    let hadOffset = true;
    server.use(
      http.get(`${BASE}/api/admin/telemetry/dev-x`, ({ request }) => {
        const sp = new URL(request.url).searchParams;
        hadLimit = sp.has('limit');
        hadOffset = sp.has('offset');
        return HttpResponse.json({ success: true, data: { records: [], total: 0 } });
      }),
    );
    const result = await adminApi.getTelemetry('dev-x');
    expect(hadLimit).toBe(false);
    expect(hadOffset).toBe(false);
    expect(result).toEqual({ records: [], total: 0 });
  });

  // ─────────── wbCenterPreview：不传 params 分支 ───────────
  it('wbCenterPreview 不传 params 时不附加 query', async () => {
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

  // ─────────── amasListSuggestions：不传 status 时仅带 limit ───────────
  it('amasListSuggestions 不传 status 时仅携带 limit（status 为 undefined 不入 URL）', async () => {
    let hadStatus = true;
    let limit: string | null = null;
    server.use(
      http.get(`${BASE}/api/admin/amas/suggestions`, ({ request }) => {
        const sp = new URL(request.url).searchParams;
        hadStatus = sp.has('status');
        limit = sp.get('limit');
        return HttpResponse.json({ success: true, data: [] });
      }),
    );
    const result = await adminApi.amasListSuggestions();
    expect(hadStatus).toBe(false);
    expect(limit).toBe('50');
    expect(result).toEqual([]);
  });

  // ─────────── amasApprove/Reject：note 为 undefined 时仍发送对象体 ───────────
  it('amasApproveSuggestion 不传 note 时 body 含 note:undefined（序列化后为空对象）', async () => {
    let raw = '';
    server.use(
      http.post(`${BASE}/api/admin/amas/suggestions/7/approve`, async ({ request }) => {
        raw = await request.text();
        return HttpResponse.json({ success: true, data: { updated: true, versionHash: 'h', versionId: 7 } });
      }),
    );
    const result = await adminApi.amasApproveSuggestion(7);
    // JSON.stringify({ note: undefined }) === '{}'
    expect(raw).toBe('{}');
    expect(result).toMatchObject({ versionId: 7 });
  });

  it('amasRejectSuggestion 传 note 时序列化进 body', async () => {
    let body: Record<string, unknown> = {};
    server.use(
      http.post(`${BASE}/api/admin/amas/suggestions/8/reject`, async ({ request }) => {
        body = (await request.json()) as Record<string, unknown>;
        return HttpResponse.json({ success: true, data: { rejected: true } });
      }),
    );
    const result = await adminApi.amasRejectSuggestion(8, '不采纳');
    expect(body).toEqual({ note: '不采纳' });
    expect(result).toEqual({ rejected: true });
  });

  // ─────────── reloadAmas 失败分支 ───────────
  it('reloadAmas 命中 400 抛 ApiError', async () => {
    server.use(
      http.post(`${BASE}/api/admin/settings/reload-amas`, () =>
        HttpResponse.json({ code: 'INVALID_CONFIG', message: '配置非法' }, { status: 400 })),
    );
    await expect(adminApi.reloadAmas({} as never)).rejects.toMatchObject({
      status: 400,
      code: 'INVALID_CONFIG',
    });
  });

  // ─────────── 204 / content-length:0 → undefined 解析 ───────────
  it('logout 在 204 无内容时解析为 undefined（不抛错）', async () => {
    server.use(
      http.post(`${BASE}/api/admin/auth/logout`, () =>
        new HttpResponse(null, { status: 204 })),
    );
    const result = await adminApi.logout();
    expect(result).toBeUndefined();
  });

  // ─────────── 非包裹体（裸 JSON）直接返回分支 ───────────
  it('verifyToken 后端返回裸对象（无 success 字段）时原样返回', async () => {
    server.use(
      http.get(`${BASE}/api/admin/auth/verify`, () =>
        HttpResponse.json({ id: 'raw-1', email: 'raw@e' })),
    );
    const result = await adminApi.verifyToken();
    expect(result).toEqual({ id: 'raw-1', email: 'raw@e' });
  });
});