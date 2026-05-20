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

  it('verifyToken returns admin identity', async () => {
    server.use(
      http.get(`${BASE}/api/admin/auth/verify`, () =>
        HttpResponse.json({ success: true, data: { id: 'a1', email: 'a@e' } })),
    );
    const result = await adminApi.verifyToken();
    expect(result).toEqual({ id: 'a1', email: 'a@e' });
  });

  it('resetUserPassword posts and returns reset key', async () => {
    server.use(
      http.post(`${BASE}/api/admin/users/u-1/reset-password`, () =>
        HttpResponse.json({ success: true, data: { resetKey: 'k', expiresInHours: 24 } })),
    );
    const result = await adminApi.resetUserPassword('u-1');
    expect(result).toEqual({ resetKey: 'k', expiresInHours: 24 });
  });

  it('setUserPassword sends new password and returns reset confirmation', async () => {
    let body: Record<string, unknown> = {};
    server.use(
      http.post(`${BASE}/api/admin/users/u-2/set-password`, async ({ request }) => {
        body = await request.json() as Record<string, unknown>;
        return HttpResponse.json({ success: true, data: { passwordReset: true, userId: 'u-2', sessionsRevoked: 3 } });
      }),
    );
    const result = await adminApi.setUserPassword('u-2', 'newpass');
    expect(body).toEqual({ newPassword: 'newpass' });
    expect(result).toEqual({ passwordReset: true, userId: 'u-2', sessionsRevoked: 3 });
  });

  it('getDailyActiveUsers without days omits param', async () => {
    server.use(
      http.get(`${BASE}/api/admin/analytics/daily-active-users`, ({ request }) => {
        const url = new URL(request.url);
        expect(url.searchParams.has('days')).toBe(false);
        return HttpResponse.json({ success: true, data: [] });
      }),
    );
    await adminApi.getDailyActiveUsers();
  });

  it('getDailyActiveUsers forwards days', async () => {
    server.use(
      http.get(`${BASE}/api/admin/analytics/daily-active-users`, ({ request }) => {
        const url = new URL(request.url);
        expect(url.searchParams.get('days')).toBe('14');
        return HttpResponse.json({ success: true, data: [{ date: '2026-05-18', activeUsers: 100 }] });
      }),
    );
    const result = await adminApi.getDailyActiveUsers(14);
    expect(result).toEqual([{ date: '2026-05-18', activeUsers: 100 }]);
  });

  it('getDailyRecords with and without days', async () => {
    server.use(
      http.get(`${BASE}/api/admin/analytics/daily-records`, ({ request }) => {
        const url = new URL(request.url);
        const days = url.searchParams.get('days');
        return HttpResponse.json({ success: true, data: days ? [{ date: '2026-05-18', count: 1 }] : [] });
      }),
    );
    expect(await adminApi.getDailyRecords()).toEqual([]);
    expect(await adminApi.getDailyRecords(7)).toEqual([{ date: '2026-05-18', count: 1 }]);
  });

  it('getStudyOverview passes both days and category', async () => {
    server.use(
      http.get(`${BASE}/api/admin/analytics/study-overview`, ({ request }) => {
        const url = new URL(request.url);
        expect(url.searchParams.get('days')).toBe('7');
        expect(url.searchParams.get('category')).toBe('cet4');
        return HttpResponse.json({ success: true, data: { totals: {} } });
      }),
    );
    await adminApi.getStudyOverview(7, 'cet4');
  });

  it('getRecordTypes/getWordStateDistribution/getRetentionCurve send their params', async () => {
    server.use(
      http.get(`${BASE}/api/admin/analytics/record-types`, ({ request }) => {
        expect(new URL(request.url).searchParams.get('days')).toBe('30');
        return HttpResponse.json({ success: true, data: { typed: 0, untyped: 0 } });
      }),
      http.get(`${BASE}/api/admin/analytics/word-states`, ({ request }) => {
        expect(new URL(request.url).searchParams.get('category')).toBe('toefl');
        return HttpResponse.json({ success: true, data: { learning: 0, mastered: 0 } });
      }),
      http.get(`${BASE}/api/admin/analytics/retention-curve`, ({ request }) => {
        // 不传 category 时，category 不应在 URL 中
        expect(new URL(request.url).searchParams.has('category')).toBe(false);
        return HttpResponse.json({ success: true, data: { points: [] } });
      }),
    );
    await adminApi.getRecordTypes(30);
    await adminApi.getWordStateDistribution('toefl');
    await adminApi.getRetentionCurve();
  });

  it('getRecordTypes/getWordStateDistribution/getRetentionCurve also support undefined-arg branch', async () => {
    server.use(
      http.get(`${BASE}/api/admin/analytics/record-types`, ({ request }) => {
        expect(new URL(request.url).searchParams.has('days')).toBe(false);
        return HttpResponse.json({ success: true, data: { typed: 0, untyped: 0 } });
      }),
      http.get(`${BASE}/api/admin/analytics/word-states`, ({ request }) => {
        expect(new URL(request.url).searchParams.has('category')).toBe(false);
        return HttpResponse.json({ success: true, data: { learning: 0, mastered: 0 } });
      }),
      http.get(`${BASE}/api/admin/analytics/retention-curve`, ({ request }) => {
        expect(new URL(request.url).searchParams.get('category')).toBe('ielts');
        return HttpResponse.json({ success: true, data: { points: [] } });
      }),
    );
    await adminApi.getRecordTypes();
    await adminApi.getWordStateDistribution();
    await adminApi.getRetentionCurve('ielts');
  });

  it('checkUpdate returns update check payload', async () => {
    server.use(
      http.get(`${BASE}/api/admin/monitoring/check-update`, () =>
        HttpResponse.json({ success: true, data: { hasUpdate: false, latestVersion: '1.0.0' } })),
    );
    expect(await adminApi.checkUpdate()).toEqual({ hasUpdate: false, latestVersion: '1.0.0' });
  });

  it('updatesStatus/updatesCheck/updatesApply hit their endpoints', async () => {
    server.use(
      http.get(`${BASE}/api/admin/updates/status`, () =>
        HttpResponse.json({ success: true, data: { state: 'idle' } })),
      http.post(`${BASE}/api/admin/updates/check`, () =>
        HttpResponse.json({ success: true, data: { state: 'checking' } })),
      http.post(`${BASE}/api/admin/updates/apply`, async ({ request }) => {
        const body = await request.json() as Record<string, unknown>;
        // v0.6.0-beta.3：channel 必填
        expect(body).toEqual({ channel: 'beta', targetVersion: '1.2.3', confirmCurrentVersion: '1.2.2' });
        return HttpResponse.json({ success: true, data: { restarting: true } });
      }),
    );
    expect(await adminApi.updatesStatus()).toEqual({ state: 'idle' });
    expect(await adminApi.updatesCheck()).toEqual({ state: 'checking' });
    expect(await adminApi.updatesApply('beta', '1.2.3', '1.2.2')).toEqual({ restarting: true });
  });

  it('updatesStatus 返回双通道 ChannelStatus 嵌套结构', async () => {
    const payload = {
      currentVersion: 'v0.6.0-beta.2',
      stable: {
        latestVersion: 'v0.5.6',
        latestPublishedAt: '2026-05-19T14:12:35Z',
        releaseNotes: 'stable notes',
        releaseUrl: 'https://example/r/v0.5.6',
        hasUpdate: false,
        canApply: false,
      },
      beta: {
        latestVersion: 'v0.6.0-beta.3',
        latestPublishedAt: '2026-05-20T20:17:07Z',
        releaseNotes: 'beta notes',
        releaseUrl: 'https://example/r/v0.6.0-beta.3',
        hasUpdate: true,
        canApply: true,
      },
      lastCheckedAt: '2026-05-20T20:30:00Z',
      autoCheckEnabled: true,
      allowDowngrade: false,
    };
    server.use(
      http.get(`${BASE}/api/admin/updates/status`, () =>
        HttpResponse.json({ success: true, data: payload })),
    );
    const result = await adminApi.updatesStatus();
    expect(result.stable?.latestVersion).toBe('v0.5.6');
    expect(result.beta?.latestVersion).toBe('v0.6.0-beta.3');
    expect(result.beta?.hasUpdate).toBe(true);
    expect(result.stable?.hasUpdate).toBe(false);
  });

  it('updatesApply 区分 stable / beta channel', async () => {
    let capturedChannel = '';
    server.use(
      http.post(`${BASE}/api/admin/updates/apply`, async ({ request }) => {
        const body = await request.json() as Record<string, unknown>;
        capturedChannel = body.channel as string;
        return HttpResponse.json({ success: true, data: { taskId: 't1', phase: 'pending', percent: 0 } });
      }),
    );
    await adminApi.updatesApply('stable', 'v0.5.6', 'v0.5.5');
    expect(capturedChannel).toBe('stable');
    await adminApi.updatesApply('beta', 'v0.6.0-beta.3', 'v0.6.0-beta.2');
    expect(capturedChannel).toBe('beta');
  });

  it('broadcastUpdate sends payload (or empty when absent)', async () => {
    let bodyA: Record<string, unknown> = {};
    let bodyB: Record<string, unknown> = {};
    server.use(
      http.post(`${BASE}/api/admin/broadcast-update`, async ({ request }) => {
        const body = await request.json() as Record<string, unknown>;
        if (body.version === '1.1.0') {
          bodyA = body;
        } else {
          bodyB = body;
        }
        return HttpResponse.json({ success: true, data: { broadcasted: true } });
      }),
    );
    await adminApi.broadcastUpdate({ version: '1.1.0', message: 'hi' });
    await adminApi.broadcastUpdate();
    expect(bodyA).toEqual({ version: '1.1.0', message: 'hi' });
    expect(bodyB).toEqual({});
  });

  it('getClients returns sseLive + recentlyActive sections', async () => {
    const payload = { sseLive: [], recentlyActive: [] };
    server.use(
      http.get(`${BASE}/api/admin/clients`, () =>
        HttpResponse.json({ success: true, data: payload })),
    );
    expect(await adminApi.getClients()).toEqual(payload);
  });

  it('banClient sends reason when provided and omits body otherwise', async () => {
    const bodies: unknown[] = [];
    server.use(
      http.post(`${BASE}/api/admin/clients/d-1/ban`, async ({ request }) => {
        const text = await request.text();
        bodies.push(text);
        return HttpResponse.json({ success: true, data: { banned: true, deviceId: 'd-1' } });
      }),
    );
    await adminApi.banClient('d-1', 'spam');
    await adminApi.banClient('d-1');
    expect(bodies[0]).toBe(JSON.stringify({ reason: 'spam' }));
    expect(bodies[1]).toBe('');
  });

  it('unbanClient hits unban endpoint', async () => {
    server.use(
      http.post(`${BASE}/api/admin/clients/d-9/unban`, () =>
        HttpResponse.json({ success: true, data: { banned: false, deviceId: 'd-9' } })),
    );
    expect(await adminApi.unbanClient('d-9')).toEqual({ banned: false, deviceId: 'd-9' });
  });

  it('requestTelemetry returns generated requestId', async () => {
    server.use(
      http.post(`${BASE}/api/admin/clients/d-2/request-telemetry`, () =>
        HttpResponse.json({ success: true, data: { requestId: 'req-1' } })),
    );
    expect(await adminApi.requestTelemetry('d-2')).toEqual({ requestId: 'req-1' });
  });

  it('getTelemetry forwards limit/offset query params', async () => {
    server.use(
      http.get(`${BASE}/api/admin/telemetry/d-3`, ({ request }) => {
        const url = new URL(request.url);
        expect(url.searchParams.get('limit')).toBe('50');
        expect(url.searchParams.get('offset')).toBe('100');
        return HttpResponse.json({ success: true, data: { records: [], total: 0 } });
      }),
    );
    await adminApi.getTelemetry('d-3', { limit: 50, offset: 100 });
  });

  it('wbCenterBrowse/Preview/Import/Updates/Sync all hit admin endpoints', async () => {
    server.use(
      http.get(`${BASE}/api/admin/wordbook-center/browse`, () =>
        HttpResponse.json({ success: true, data: [] })),
      http.get(`${BASE}/api/admin/wordbook-center/browse/wb-1`, ({ request }) => {
        expect(new URL(request.url).searchParams.get('page')).toBe('1');
        return HttpResponse.json({ success: true, data: { id: 'wb-1', title: 't', words: [], total: 0 } });
      }),
      http.post(`${BASE}/api/admin/wordbook-center/import/wb-1`, () =>
        HttpResponse.json({ success: true, data: { imported: 1, skipped: 0, errors: [] } })),
      http.get(`${BASE}/api/admin/wordbook-center/updates`, () =>
        HttpResponse.json({ success: true, data: [] })),
      http.post(`${BASE}/api/admin/wordbook-center/updates/wb-1/sync`, () =>
        HttpResponse.json({ success: true, data: { updated: 1, added: 0, removed: 0 } })),
    );
    expect(await adminApi.wbCenterBrowse()).toEqual([]);
    expect(await adminApi.wbCenterPreview('wb-1', { page: 1 })).toMatchObject({ id: 'wb-1' });
    expect(await adminApi.wbCenterImport('wb-1')).toMatchObject({ imported: 1 });
    expect(await adminApi.wbCenterUpdates()).toEqual([]);
    expect(await adminApi.wbCenterSync('wb-1')).toMatchObject({ updated: 1 });
  });

  it('amasListVersions sends default limit and amasGetVersion fetches by hash', async () => {
    server.use(
      http.get(`${BASE}/api/admin/amas/config/versions`, ({ request }) => {
        expect(new URL(request.url).searchParams.get('limit')).toBe('50');
        return HttpResponse.json({ success: true, data: [{ id: 1, versionHash: 'h1' }] });
      }),
      http.get(`${BASE}/api/admin/amas/config/versions/h1`, () =>
        HttpResponse.json({ success: true, data: { id: 1, versionHash: 'h1', snapshotJson: {} } })),
    );
    expect(await adminApi.amasListVersions()).toEqual([{ id: 1, versionHash: 'h1' }]);
    expect(await adminApi.amasGetVersion('h1')).toMatchObject({ versionHash: 'h1' });
  });

  it('amasRestoreVersion posts note and returns updated version', async () => {
    let body: Record<string, unknown> = {};
    server.use(
      http.post(`${BASE}/api/admin/amas/config/versions/h2/restore`, async ({ request }) => {
        body = await request.json() as Record<string, unknown>;
        return HttpResponse.json({ success: true, data: { updated: true, versionHash: 'h2', versionId: 2 } });
      }),
    );
    await adminApi.amasRestoreVersion('h2', 'reverting');
    expect(body).toEqual({ note: 'reverting' });
  });

  it('amasUpdateConfigWithNote appends note query when provided', async () => {
    let queryNote: string | null = null;
    server.use(
      http.put(`${BASE}/api/admin/amas/config`, ({ request }) => {
        queryNote = new URL(request.url).searchParams.get('note');
        return HttpResponse.json({ success: true, data: { updated: true, versionHash: 'h3', versionId: 3 } });
      }),
    );
    await adminApi.amasUpdateConfigWithNote({} as any, 'tune & cap');
    expect(queryNote).toBe('tune & cap');
  });

  it('amasUpdateConfigWithNote omits note query when undefined', async () => {
    let hadNote = true;
    server.use(
      http.put(`${BASE}/api/admin/amas/config`, ({ request }) => {
        hadNote = new URL(request.url).searchParams.has('note');
        return HttpResponse.json({ success: true, data: { updated: true, versionHash: 'h4', versionId: 4 } });
      }),
    );
    await adminApi.amasUpdateConfigWithNote({} as any);
    expect(hadNote).toBe(false);
  });

  it('amas metrics/anomalies/distribution/compare each pass their default query params', async () => {
    server.use(
      http.get(`${BASE}/api/admin/amas/metrics/timeseries`, ({ request }) => {
        expect(new URL(request.url).searchParams.get('days')).toBe('7');
        return HttpResponse.json({ success: true, data: [] });
      }),
      http.get(`${BASE}/api/admin/amas/anomalies`, ({ request }) => {
        expect(new URL(request.url).searchParams.get('days')).toBe('14');
        return HttpResponse.json({ success: true, data: { totalEvents: 0, anomalyCount: 0, violationCount: 0, coldStartExplore: 0, coldStartExploit: 0, byDay: [], topViolationFields: [] } });
      }),
      http.get(`${BASE}/api/admin/amas/user-state/distribution`, ({ request }) => {
        const url = new URL(request.url);
        expect(url.searchParams.get('days')).toBe('2');
        expect(url.searchParams.get('bins')).toBe('30');
        return HttpResponse.json({ success: true, data: {} });
      }),
      http.get(`${BASE}/api/admin/amas/compare`, ({ request }) => {
        const url = new URL(request.url);
        expect(url.searchParams.get('versionA')).toBe('A');
        expect(url.searchParams.get('versionB')).toBe('B');
        return HttpResponse.json({ success: true, data: { a: {}, b: {} } });
      }),
    );
    await adminApi.amasMetricsTimeseries();
    await adminApi.amasAnomaliesOverview(14);
    await adminApi.amasUserStateDistribution(2, 30);
    await adminApi.amasCompareVersions('A', 'B');
  });

  it('amas suggestions: list/get/approve/reject/explain/spend round-trip', async () => {
    server.use(
      http.get(`${BASE}/api/admin/amas/suggestions`, ({ request }) => {
        const url = new URL(request.url);
        expect(url.searchParams.get('status')).toBe('pending');
        expect(url.searchParams.get('limit')).toBe('20');
        return HttpResponse.json({ success: true, data: [{ id: 1 }] });
      }),
      http.get(`${BASE}/api/admin/amas/suggestions/1`, () =>
        HttpResponse.json({ success: true, data: { id: 1 } })),
      http.post(`${BASE}/api/admin/amas/suggestions/1/approve`, async ({ request }) => {
        const body = await request.json() as Record<string, unknown>;
        expect(body).toEqual({ note: 'lgtm' });
        return HttpResponse.json({ success: true, data: { updated: true, versionHash: 'h', versionId: 1 } });
      }),
      http.post(`${BASE}/api/admin/amas/suggestions/2/reject`, async ({ request }) => {
        const body = await request.json() as Record<string, unknown>;
        expect(body).toEqual({ note: 'nope' });
        return HttpResponse.json({ success: true, data: { rejected: true } });
      }),
      http.post(`${BASE}/api/admin/amas/suggestions/explain`, async ({ request }) => {
        const body = await request.json() as Record<string, unknown>;
        expect(body).toEqual({ path: 'modeling.attentionSmoothing', currentValue: 0.3 });
        return HttpResponse.json({ success: true, data: { explanation: 'x', model: 'm', costUsd: 0, tokensInput: 0, tokensOutput: 0 } });
      }),
      http.get(`${BASE}/api/admin/amas/suggestions/spend`, () =>
        HttpResponse.json({ success: true, data: { todayCostUsd: 0, todayTokensInput: 0, todayTokensOutput: 0, dailyCapUsd: 1, remainingUsd: 1 } })),
    );
    expect(await adminApi.amasListSuggestions('pending', 20)).toEqual([{ id: 1 }]);
    expect(await adminApi.amasGetSuggestion(1)).toEqual({ id: 1 });
    expect(await adminApi.amasApproveSuggestion(1, 'lgtm')).toMatchObject({ updated: true });
    expect(await adminApi.amasRejectSuggestion(2, 'nope')).toEqual({ rejected: true });
    expect(await adminApi.amasExplainParam('modeling.attentionSmoothing', 0.3)).toMatchObject({ model: 'm' });
    expect(await adminApi.amasSuggestionSpend()).toMatchObject({ remainingUsd: 1 });
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

  // ─────────── 用户反馈契约（防 v0.6.0-beta.1 ErrorBoundary 回归） ───────────
  // 历史教训：admin.ts 曾把 listFeedback 类型写成 `items: FeedbackItem[]`，
  // 而后端 paginated() 返回 `data:{ data, total, page, perPage, totalPages }`，
  // 运行时 resp.items === undefined → setItems(undefined) → undefined.length 抛错
  // → AppErrorBoundary 全屏接住"出错了"。本组用例锁定契约。
  it('listFeedback unwraps paginated payload with `data` field (regression)', async () => {
    const feedbackItem = {
      id: 'fb-1',
      userId: 'u-1',
      category: 'profile',
      body: '请改进复习统计',
      route: '/profile',
      createdAt: '2026-05-20T11:00:00Z',
    };
    const payload = {
      data: [feedbackItem],
      total: 1,
      page: 1,
      perPage: 20,
      totalPages: 1,
    };
    server.use(
      http.get(`${BASE}/api/admin/feedback`, ({ request }) => {
        const url = new URL(request.url);
        expect(url.searchParams.get('page')).toBe('1');
        expect(url.searchParams.get('perPage')).toBe('20');
        return HttpResponse.json({ success: true, data: payload });
      }),
    );
    const result = await adminApi.listFeedback({ page: 1, perPage: 20 });
    // 列表字段必须是 `data` 而非 `items`
    expect(result.data).toEqual([feedbackItem]);
    expect((result as unknown as { items?: unknown }).items).toBeUndefined();
    expect(result.total).toBe(1);
    expect(result.page).toBe(1);
    expect(result.perPage).toBe(20);
    expect(result.totalPages).toBe(1);
  });

  it('listFeedback handles empty list without throwing', async () => {
    server.use(
      http.get(`${BASE}/api/admin/feedback`, () =>
        HttpResponse.json({
          success: true,
          data: { data: [], total: 0, page: 1, perPage: 20, totalPages: 0 },
        })),
    );
    const result = await adminApi.listFeedback({ page: 1, perPage: 20 });
    expect(Array.isArray(result.data)).toBe(true);
    expect(result.data).toHaveLength(0);
    expect(result.total).toBe(0);
  });
});
