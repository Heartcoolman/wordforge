import { describe, it, expect, vi, beforeAll, afterAll, afterEach } from 'vitest';
import { setupServer } from 'msw/node';
import { http, HttpResponse } from 'msw';

import { TEST_BASE_URL as BASE } from './helpers/constants';

// 覆盖域：users_analytics —— adminApi 中 users（增删改查/封禁/批量/profile/
// sessions/devices/audit-log/activity-log/extras/facets）、analytics（engagement/
// learning/daily-active-users/daily-records/study-overview/record-types/
// word-states/retention-curve + KPI/漏斗/cohort/题型/高频词/洞察/hourly/rank）、stats。
// 全程 MSW 拦截，绝不真连网络，逐方法验证 URL 拼接、query/body 序列化、data 解析。

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

// 通用 envelope helper
const ok = (data: unknown) => HttpResponse.json({ success: true, data });

describe('adminApi users_analytics —— users 域', () => {
  it('getStats 命中 GET /stats 并解析', async () => {
    const stats = { users: 100, words: 5000, records: 8000 };
    server.use(http.get(`${BASE}/api/admin/stats`, () => ok(stats)));
    expect(await adminApi.getStats()).toEqual(stats);
  });

  it('getUsers 无参时 URL 无 query', async () => {
    let qs = '';
    server.use(
      http.get(`${BASE}/api/admin/users`, ({ request }) => {
        qs = new URL(request.url).search;
        return ok({ data: [], total: 0, page: 1, perPage: 20, totalPages: 0 });
      }),
    );
    const r = await adminApi.getUsers();
    expect(qs).toBe('');
    expect(r.total).toBe(0);
  });

  it('getUsers 透传 query（page/perPage/q/role/status）', async () => {
    let url: URL | null = null;
    server.use(
      http.get(`${BASE}/api/admin/users`, ({ request }) => {
        url = new URL(request.url);
        return ok({ data: [{ id: 'u1' }], total: 1, page: 2, perPage: 10, totalPages: 1 });
      }),
    );
    const r = await adminApi.getUsers({ page: 2, perPage: 10, q: 'bob', role: 'staff', status: 'active' } as any);
    expect(url!.searchParams.get('page')).toBe('2');
    expect(url!.searchParams.get('perPage')).toBe('10');
    expect(url!.searchParams.get('q')).toBe('bob');
    expect(url!.searchParams.get('role')).toBe('staff');
    expect(url!.searchParams.get('status')).toBe('active');
    expect(r.page).toBe(2);
  });

  it('createUser POST body 透传 payload', async () => {
    let body: Record<string, unknown> = {};
    const created = { id: 'u-new', email: 'n@e.com', username: 'newbie' };
    server.use(
      http.post(`${BASE}/api/admin/users`, async ({ request }) => {
        body = (await request.json()) as Record<string, unknown>;
        return ok(created);
      }),
    );
    const r = await adminApi.createUser({ email: 'n@e.com', username: 'newbie', password: 'pw' } as any);
    expect(body).toEqual({ email: 'n@e.com', username: 'newbie', password: 'pw' });
    expect(r).toEqual(created);
  });

  it('banUser / unbanUser 命中各自端点', async () => {
    server.use(
      http.post(`${BASE}/api/admin/users/u-7/ban`, () => ok({ banned: true, userId: 'u-7' })),
      http.post(`${BASE}/api/admin/users/u-7/unban`, () => ok({ banned: false, userId: 'u-7' })),
    );
    expect(await adminApi.banUser('u-7')).toEqual({ banned: true, userId: 'u-7' });
    expect(await adminApi.unbanUser('u-7')).toEqual({ banned: false, userId: 'u-7' });
  });

  it('resetUserPassword 命中 reset-password', async () => {
    server.use(
      http.post(`${BASE}/api/admin/users/u-1/reset-password`, () =>
        ok({ resetKey: 'k', expiresInHours: 24 })),
    );
    expect(await adminApi.resetUserPassword('u-1')).toEqual({ resetKey: 'k', expiresInHours: 24 });
  });

  it('setUserPassword POST body { newPassword }', async () => {
    let body: Record<string, unknown> = {};
    server.use(
      http.post(`${BASE}/api/admin/users/u-2/set-password`, async ({ request }) => {
        body = (await request.json()) as Record<string, unknown>;
        return ok({ passwordReset: true, userId: 'u-2', sessionsRevoked: 2 });
      }),
    );
    const r = await adminApi.setUserPassword('u-2', 'np');
    expect(body).toEqual({ newPassword: 'np' });
    expect(r.sessionsRevoked).toBe(2);
  });

  it('patchUserRole PATCH body { role }', async () => {
    let body: Record<string, unknown> = {};
    let method = '';
    server.use(
      http.patch(`${BASE}/api/admin/users/u-3/role`, async ({ request }) => {
        method = request.method;
        body = (await request.json()) as Record<string, unknown>;
        return ok({ id: 'u-3', role: 'admin' });
      }),
    );
    const r = await adminApi.patchUserRole('u-3', 'admin');
    expect(method).toBe('PATCH');
    expect(body).toEqual({ role: 'admin' });
    expect(r).toMatchObject({ id: 'u-3' });
  });

  it('usersBulkRole POST body { userIds, role }', async () => {
    let body: Record<string, unknown> = {};
    server.use(
      http.post(`${BASE}/api/admin/users/bulk-role`, async ({ request }) => {
        body = (await request.json()) as Record<string, unknown>;
        return ok({ total: 2, succeeded: 2, failed: 0, results: [], role: 'staff' });
      }),
    );
    const r = await adminApi.usersBulkRole(['a', 'b'], 'staff');
    expect(body).toEqual({ userIds: ['a', 'b'], role: 'staff' });
    expect(r.succeeded).toBe(2);
  });

  it('userProfile 命中 /users/:id/profile', async () => {
    const profile = { userId: 'u-9', totalRecords: 10, correctRecords: 8, accuracy: 0.8, avgResponseTimeMs: 1200, sessionCount: 3, wordbookDistribution: [] };
    server.use(http.get(`${BASE}/api/admin/users/u-9/profile`, () => ok(profile)));
    expect(await adminApi.userProfile('u-9')).toEqual(profile);
  });

  it('userSessions 默认 limit=20，自定义 limit 透传', async () => {
    const limits: (string | null)[] = [];
    server.use(
      http.get(`${BASE}/api/admin/users/u-1/sessions`, ({ request }) => {
        limits.push(new URL(request.url).searchParams.get('limit'));
        return ok({ sessions: [] });
      }),
    );
    await adminApi.userSessions('u-1');
    await adminApi.userSessions('u-1', 5);
    expect(limits).toEqual(['20', '5']);
  });

  it('usersBulkBan POST body 带 reason / 省略 reason', async () => {
    const bodies: Record<string, unknown>[] = [];
    server.use(
      http.post(`${BASE}/api/admin/users/bulk-ban`, async ({ request }) => {
        bodies.push((await request.json()) as Record<string, unknown>);
        return ok({ total: 1, succeeded: 1, failed: 0, results: [] });
      }),
    );
    await adminApi.usersBulkBan(['x'], 'spam');
    await adminApi.usersBulkBan(['y']);
    expect(bodies[0]).toEqual({ userIds: ['x'], reason: 'spam' });
    // reason 省略时 JSON 序列化为 undefined -> 不出现在 body
    expect(bodies[1]).toEqual({ userIds: ['y'] });
  });

  it('usersBulkUnban POST body { userIds }', async () => {
    let body: Record<string, unknown> = {};
    server.use(
      http.post(`${BASE}/api/admin/users/bulk-unban`, async ({ request }) => {
        body = (await request.json()) as Record<string, unknown>;
        return ok({ total: 1, succeeded: 1, failed: 0, results: [] });
      }),
    );
    await adminApi.usersBulkUnban(['z']);
    expect(body).toEqual({ userIds: ['z'] });
  });

  it('userDevices 默认 limit=20', async () => {
    let limit: string | null = null;
    server.use(
      http.get(`${BASE}/api/admin/users/u-2/devices`, ({ request }) => {
        limit = new URL(request.url).searchParams.get('limit');
        return ok({ devices: [] });
      }),
    );
    const r = await adminApi.userDevices('u-2');
    expect(limit).toBe('20');
    expect(r.devices).toEqual([]);
  });

  it('userAuditLog 默认 limit=50', async () => {
    let limit: string | null = null;
    server.use(
      http.get(`${BASE}/api/admin/users/u-3/audit-log`, ({ request }) => {
        limit = new URL(request.url).searchParams.get('limit');
        return ok({ entries: [] });
      }),
    );
    const r = await adminApi.userAuditLog('u-3');
    expect(limit).toBe('50');
    expect(r.entries).toEqual([]);
  });

  it('userExtras 命中 /users/:id/extras', async () => {
    const extras = { preferences: {}, elo: 1500 };
    server.use(http.get(`${BASE}/api/admin/users/u-4/extras`, () => ok(extras)));
    expect(await adminApi.userExtras('u-4')).toEqual(extras);
  });

  it('userActivityLog 默认 limit=50，自定义透传', async () => {
    const limits: (string | null)[] = [];
    server.use(
      http.get(`${BASE}/api/admin/users/u-5/activity-log`, ({ request }) => {
        limits.push(new URL(request.url).searchParams.get('limit'));
        return ok({ entries: [] });
      }),
    );
    await adminApi.userActivityLog('u-5');
    await adminApi.userActivityLog('u-5', 100);
    expect(limits).toEqual(['50', '100']);
  });

  it('usersBulkResetPassword POST body { userIds }', async () => {
    let body: Record<string, unknown> = {};
    server.use(
      http.post(`${BASE}/api/admin/users/bulk-reset-password`, async ({ request }) => {
        body = (await request.json()) as Record<string, unknown>;
        return ok({ total: 2, succeeded: 2, failed: 0, results: [] });
      }),
    );
    await adminApi.usersBulkResetPassword(['a', 'b']);
    expect(body).toEqual({ userIds: ['a', 'b'] });
  });

  it('usersBulkDelete POST body { userIds, reason }', async () => {
    let body: Record<string, unknown> = {};
    server.use(
      http.post(`${BASE}/api/admin/users/bulk-delete`, async ({ request }) => {
        body = (await request.json()) as Record<string, unknown>;
        return ok({ total: 1, succeeded: 1, failed: 0, results: [] });
      }),
    );
    await adminApi.usersBulkDelete(['d'], 'gdpr');
    expect(body).toEqual({ userIds: ['d'], reason: 'gdpr' });
  });

  it('banUserDevice 命中 /users/:uid/devices/:did/ban', async () => {
    let method = '';
    server.use(
      http.post(`${BASE}/api/admin/users/u-6/devices/dev-1/ban`, ({ request }) => {
        method = request.method;
        return ok({ banned: true, deviceId: 'dev-1' });
      }),
    );
    const r = await adminApi.banUserDevice('u-6', 'dev-1');
    expect(method).toBe('POST');
    expect(r).toEqual({ banned: true, deviceId: 'dev-1' });
  });

  it('userFacets 命中 /users/facets', async () => {
    const facets = { total: 100, active: 80, banned: 5, staff: 2 };
    server.use(http.get(`${BASE}/api/admin/users/facets`, () => ok(facets)));
    expect(await adminApi.userFacets()).toEqual(facets);
  });
});

describe('adminApi users_analytics —— analytics 域', () => {
  it('getEngagement 命中 /analytics/engagement', async () => {
    const e = { totalUsers: 100, activeToday: 30, retentionRate: 0.3 };
    server.use(http.get(`${BASE}/api/admin/analytics/engagement`, () => ok(e)));
    expect(await adminApi.getEngagement()).toEqual(e);
  });

  it('getLearningAnalytics 命中 /analytics/learning', async () => {
    const l = { totalWords: 5000, totalRecords: 10000, totalCorrect: 8500, overallAccuracy: 0.85 };
    server.use(http.get(`${BASE}/api/admin/analytics/learning`, () => ok(l)));
    expect(await adminApi.getLearningAnalytics()).toEqual(l);
  });

  it('getDailyActiveUsers 省略/传 days', async () => {
    const seen: (string | null)[] = [];
    server.use(
      http.get(`${BASE}/api/admin/analytics/daily-active-users`, ({ request }) => {
        seen.push(new URL(request.url).searchParams.get('days'));
        return ok([{ date: '2026-06-01', activeUsers: 9 }]);
      }),
    );
    await adminApi.getDailyActiveUsers();
    await adminApi.getDailyActiveUsers(30);
    expect(seen).toEqual([null, '30']);
  });

  it('getDailyRecords 省略/传 days', async () => {
    const seen: (string | null)[] = [];
    server.use(
      http.get(`${BASE}/api/admin/analytics/daily-records`, ({ request }) => {
        seen.push(new URL(request.url).searchParams.get('days'));
        return ok([]);
      }),
    );
    await adminApi.getDailyRecords();
    await adminApi.getDailyRecords(7);
    expect(seen).toEqual([null, '7']);
  });

  it('getStudyOverview 同时省略两参 / 仅 days / days+category', async () => {
    const calls: { days: string | null; cat: string | null }[] = [];
    server.use(
      http.get(`${BASE}/api/admin/analytics/study-overview`, ({ request }) => {
        const u = new URL(request.url);
        calls.push({ days: u.searchParams.get('days'), cat: u.searchParams.get('category') });
        return ok({ totals: {} });
      }),
    );
    await adminApi.getStudyOverview();
    await adminApi.getStudyOverview(7);
    await adminApi.getStudyOverview(14, 'cet4');
    expect(calls[0]).toEqual({ days: null, cat: null });
    expect(calls[1]).toEqual({ days: '7', cat: null });
    expect(calls[2]).toEqual({ days: '14', cat: 'cet4' });
  });

  it('getRecordTypes 省略/传 days', async () => {
    const seen: (string | null)[] = [];
    server.use(
      http.get(`${BASE}/api/admin/analytics/record-types`, ({ request }) => {
        seen.push(new URL(request.url).searchParams.get('days'));
        return ok({ typed: 0, untyped: 0 });
      }),
    );
    await adminApi.getRecordTypes();
    await adminApi.getRecordTypes(30);
    expect(seen).toEqual([null, '30']);
  });

  it('getWordStateDistribution 省略/传 category', async () => {
    const seen: (string | null)[] = [];
    server.use(
      http.get(`${BASE}/api/admin/analytics/word-states`, ({ request }) => {
        seen.push(new URL(request.url).searchParams.get('category'));
        return ok({ learning: 0, mastered: 0 });
      }),
    );
    await adminApi.getWordStateDistribution();
    await adminApi.getWordStateDistribution('toefl');
    expect(seen).toEqual([null, 'toefl']);
  });

  it('getRetentionCurve 省略/传 category', async () => {
    const seen: (string | null)[] = [];
    server.use(
      http.get(`${BASE}/api/admin/analytics/retention-curve`, ({ request }) => {
        seen.push(new URL(request.url).searchParams.get('category'));
        return ok({ points: [] });
      }),
    );
    await adminApi.getRetentionCurve();
    await adminApi.getRetentionCurve('ielts');
    expect(seen).toEqual([null, 'ielts']);
  });

  it('analyticsHourly 默认 days=7', async () => {
    let days: string | null = null;
    server.use(
      http.get(`${BASE}/api/admin/analytics/hourly`, ({ request }) => {
        days = new URL(request.url).searchParams.get('days');
        return ok({ generatedAt: 't', days: 7, matrix: [], total: 0 });
      }),
    );
    const r = await adminApi.analyticsHourly();
    expect(days).toBe('7');
    expect(r.total).toBe(0);
  });

  it('analyticsWordbookRank 透传 days/limit，省略时无 query', async () => {
    const calls: string[] = [];
    server.use(
      http.get(`${BASE}/api/admin/analytics/wordbook-rank`, ({ request }) => {
        calls.push(new URL(request.url).search);
        return ok({ generatedAt: 't', days: 7, limit: 10, rows: [] });
      }),
    );
    await adminApi.analyticsWordbookRank();
    await adminApi.analyticsWordbookRank({ days: 30, limit: 5 });
    expect(calls[0]).toBe('');
    const u = new URL(`${BASE}/x${calls[1]}`);
    expect(u.searchParams.get('days')).toBe('30');
    expect(u.searchParams.get('limit')).toBe('5');
  });

  it('analyticsRetentionCohort 透传 cohort/maxDays', async () => {
    let u: URL | null = null;
    server.use(
      http.get(`${BASE}/api/admin/analytics/retention-cohort`, ({ request }) => {
        u = new URL(request.url);
        return ok({ generatedAt: 't', cohortUnit: 'weekly', maxDays: 28, rows: [] });
      }),
    );
    const r = await adminApi.analyticsRetentionCohort({ cohort: 'weekly', maxDays: 28 });
    expect(u!.searchParams.get('cohort')).toBe('weekly');
    expect(u!.searchParams.get('maxDays')).toBe('28');
    expect(r.cohortUnit).toBe('weekly');
  });

  it('analyticsKpiSummary 透传 days/from/to', async () => {
    let u: URL | null = null;
    server.use(
      http.get(`${BASE}/api/admin/analytics/kpi-summary`, ({ request }) => {
        u = new URL(request.url);
        return ok({ ok: true });
      }),
    );
    await adminApi.analyticsKpiSummary({ days: 14, from: '2026-05-01', to: '2026-05-15' });
    expect(u!.searchParams.get('days')).toBe('14');
    expect(u!.searchParams.get('from')).toBe('2026-05-01');
    expect(u!.searchParams.get('to')).toBe('2026-05-15');
  });

  it('analyticsFunnel 命中 /analytics/funnel', async () => {
    server.use(http.get(`${BASE}/api/admin/analytics/funnel`, () => ok({ stages: [] })));
    expect(await adminApi.analyticsFunnel()).toEqual({ stages: [] });
  });

  it('analyticsRetentionMatrix 默认 weeks=7', async () => {
    let weeks: string | null = null;
    server.use(
      http.get(`${BASE}/api/admin/analytics/retention-matrix`, ({ request }) => {
        weeks = new URL(request.url).searchParams.get('weeks');
        return ok({ matrix: [] });
      }),
    );
    await adminApi.analyticsRetentionMatrix();
    expect(weeks).toBe('7');
  });

  it('analyticsQuestionDistribution 命中端点', async () => {
    server.use(http.get(`${BASE}/api/admin/analytics/question-distribution`, () => ok({ types: [] })));
    expect(await adminApi.analyticsQuestionDistribution()).toEqual({ types: [] });
  });

  it('analyticsWordFrequency 透传 sort 等参数', async () => {
    let u: URL | null = null;
    server.use(
      http.get(`${BASE}/api/admin/analytics/word-frequency`, ({ request }) => {
        u = new URL(request.url);
        return ok({ words: [] });
      }),
    );
    await adminApi.analyticsWordFrequency({ days: 7, limit: 20, sort: 'accuracy' });
    expect(u!.searchParams.get('sort')).toBe('accuracy');
    expect(u!.searchParams.get('limit')).toBe('20');
  });

  it('analyticsInsights 命中 /analytics/insights', async () => {
    server.use(http.get(`${BASE}/api/admin/analytics/insights`, () => ok({ items: [] })));
    expect(await adminApi.analyticsInsights()).toEqual({ items: [] });
  });
});
