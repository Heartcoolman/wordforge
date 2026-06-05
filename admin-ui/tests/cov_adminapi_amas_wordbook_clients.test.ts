// 覆盖率补强：adminApi 中 AMAS / 词库中心 / 本地词库 / 设备(clients) 三域方法。
// 严格沿用 tests/api/admin.test.ts 的 msw 范式：setupServer + http.<verb> 拦截，
// 断言 URL 拼接 / query 序列化 / body 序列化 / data 解析。
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
  },
}));
vi.mock('@/api/auth', () => ({ authApi: { refresh: vi.fn() } }));

import { adminApi } from '@/api/admin';

const ok = (data: unknown) => HttpResponse.json({ success: true, data });

describe('adminApi · AMAS config / versions', () => {
  it('amasListVersions sends limit query', async () => {
    server.use(http.get(`${BASE}/api/admin/amas/config/versions`, ({ request }) => {
      expect(new URL(request.url).searchParams.get('limit')).toBe('25');
      return ok([{ id: 1, versionHash: 'h1' }]);
    }));
    expect(await adminApi.amasListVersions(25)).toEqual([{ id: 1, versionHash: 'h1' }]);
  });

  it('amasListVersions default limit = 50', async () => {
    server.use(http.get(`${BASE}/api/admin/amas/config/versions`, ({ request }) => {
      expect(new URL(request.url).searchParams.get('limit')).toBe('50');
      return ok([]);
    }));
    expect(await adminApi.amasListVersions()).toEqual([]);
  });

  it('amasGetVersion fetches by hash', async () => {
    server.use(http.get(`${BASE}/api/admin/amas/config/versions/abc`, () =>
      ok({ id: 2, versionHash: 'abc', snapshotJson: {} })));
    expect(await adminApi.amasGetVersion('abc')).toMatchObject({ versionHash: 'abc' });
  });

  it('amasRestoreVersion posts note', async () => {
    let body: Record<string, unknown> = {};
    server.use(http.post(`${BASE}/api/admin/amas/config/versions/h2/restore`, async ({ request }) => {
      body = await request.json() as Record<string, unknown>;
      return ok({ updated: true, versionHash: 'h2', versionId: 2 });
    }));
    expect(await adminApi.amasRestoreVersion('h2', 'note-x')).toMatchObject({ updated: true });
    expect(body).toEqual({ note: 'note-x' });
  });

  it('amasUpdateConfigWithNote appends note query when present and serializes config body', async () => {
    let note: string | null = null;
    let body: unknown = null;
    server.use(http.put(`${BASE}/api/admin/amas/config`, async ({ request }) => {
      note = new URL(request.url).searchParams.get('note');
      body = await request.json();
      return ok({ updated: true, versionHash: 'h3', versionId: 3 });
    }));
    await adminApi.amasUpdateConfigWithNote({ foo: 1 } as any, 'tune & cap');
    expect(note).toBe('tune & cap');
    expect(body).toEqual({ foo: 1 });
  });

  it('amasUpdateConfigWithNote omits note query when undefined', async () => {
    let hadNote = true;
    server.use(http.put(`${BASE}/api/admin/amas/config`, ({ request }) => {
      hadNote = new URL(request.url).searchParams.has('note');
      return ok({ updated: true, versionHash: 'h4', versionId: 4 });
    }));
    await adminApi.amasUpdateConfigWithNote({} as any);
    expect(hadNote).toBe(false);
  });

  it('amasParseToml posts { toml } and returns config', async () => {
    let body: Record<string, unknown> = {};
    server.use(http.post(`${BASE}/api/admin/amas/config/parse-toml`, async ({ request }) => {
      body = await request.json() as Record<string, unknown>;
      return ok({ parsed: true });
    }));
    expect(await adminApi.amasParseToml('a = 1')).toEqual({ parsed: true });
    expect(body).toEqual({ toml: 'a = 1' });
  });

  it('amasSerializeToml posts config and returns { toml }', async () => {
    let body: unknown = null;
    server.use(http.post(`${BASE}/api/admin/amas/config/serialize-toml`, async ({ request }) => {
      body = await request.json();
      return ok({ toml: 'a = 1' });
    }));
    expect(await adminApi.amasSerializeToml({ a: 1 } as any)).toEqual({ toml: 'a = 1' });
    expect(body).toEqual({ a: 1 });
  });

  it('amasGetCanary returns canary or null', async () => {
    server.use(http.get(`${BASE}/api/admin/amas/config/canary`, () => ok({ canary: null })));
    expect(await adminApi.amasGetCanary()).toEqual({ canary: null });
  });

  it('amasSetCanary puts payload', async () => {
    let body: Record<string, unknown> = {};
    server.use(http.put(`${BASE}/api/admin/amas/config/canary`, async ({ request }) => {
      body = await request.json() as Record<string, unknown>;
      return ok({ canary: { id: 1, versionHash: 'h', percent: 10, forceUserIds: [], createdAt: 't', createdBy: 'a' } });
    }));
    await adminApi.amasSetCanary({ versionHash: 'h', percent: 10, forceUserIds: ['u1'] });
    expect(body).toEqual({ versionHash: 'h', percent: 10, forceUserIds: ['u1'] });
  });

  it('amasSetCanaryExt puts crowd-filter payload to same endpoint', async () => {
    let body: unknown = null;
    server.use(http.put(`${BASE}/api/admin/amas/config/canary`, async ({ request }) => {
      body = await request.json();
      return ok({ canary: {}, audience: 5, crowdFilters: {} });
    }));
    const res = await adminApi.amasSetCanaryExt({ versionHash: 'h', percent: 20 } as any);
    expect(res).toMatchObject({ audience: 5 });
    expect(body).toMatchObject({ versionHash: 'h' });
  });

  it('amasDisableCanary posts to disable endpoint', async () => {
    server.use(http.post(`${BASE}/api/admin/amas/config/canary/disable`, () => ok({ disabled: true })));
    expect(await adminApi.amasDisableCanary()).toEqual({ disabled: true });
  });

  it('amasConfigDiffImpact posts { patch }', async () => {
    let body: Record<string, unknown> = {};
    server.use(http.post(`${BASE}/api/admin/amas/config/diff-impact`, async ({ request }) => {
      body = await request.json() as Record<string, unknown>;
      return ok({ deltas: [] });
    }));
    await adminApi.amasConfigDiffImpact({ 'modeling.x': 0.5 });
    expect(body).toEqual({ patch: { 'modeling.x': 0.5 } });
  });
});

describe('adminApi · AMAS metrics / anomalies / user-state / compare', () => {
  it('amasMetricsTimeseries default + custom days', async () => {
    server.use(http.get(`${BASE}/api/admin/amas/metrics/timeseries`, ({ request }) => {
      return ok([{ days: new URL(request.url).searchParams.get('days') }]);
    }));
    expect(await adminApi.amasMetricsTimeseries()).toEqual([{ days: '7' }]);
    expect(await adminApi.amasMetricsTimeseries(14)).toEqual([{ days: '14' }]);
  });

  it('amasMetricsKpi sends days', async () => {
    server.use(http.get(`${BASE}/api/admin/amas/metrics/kpi`, ({ request }) => {
      expect(new URL(request.url).searchParams.get('days')).toBe('30');
      return ok({ decisionTotal: 1 });
    }));
    expect(await adminApi.amasMetricsKpi(30)).toMatchObject({ decisionTotal: 1 });
  });

  it('amasAlgorithmDistribution sends days', async () => {
    server.use(http.get(`${BASE}/api/admin/amas/metrics/algorithm-distribution`, ({ request }) => {
      expect(new URL(request.url).searchParams.get('days')).toBe('7');
      return ok([{ algorithm: 'sm2', count: 3, pct: 0.5 }]);
    }));
    expect(await adminApi.amasAlgorithmDistribution()).toEqual([{ algorithm: 'sm2', count: 3, pct: 0.5 }]);
  });

  it('amasStageDistribution hits stage-distribution', async () => {
    server.use(http.get(`${BASE}/api/admin/amas/metrics/stage-distribution`, () =>
      ok({ totalUsers: 9, stages: [], trend: [] })));
    expect(await adminApi.amasStageDistribution()).toMatchObject({ totalUsers: 9 });
  });

  it('amasEloScatter sends limit', async () => {
    server.use(http.get(`${BASE}/api/admin/amas/metrics/elo-scatter`, ({ request }) => {
      expect(new URL(request.url).searchParams.get('limit')).toBe('400');
      return ok({ points: [], total: 0, meanElo: 1500 });
    }));
    expect(await adminApi.amasEloScatter()).toMatchObject({ meanElo: 1500 });
  });

  it('amasMdmHeatmap sends days', async () => {
    server.use(http.get(`${BASE}/api/admin/amas/metrics/mdm-heatmap`, ({ request }) => {
      expect(new URL(request.url).searchParams.get('days')).toBe('7');
      return ok({ days: [], bandCount: 14, cells: [], peak: 0 });
    }));
    expect(await adminApi.amasMdmHeatmap()).toMatchObject({ bandCount: 14 });
  });

  it('amasFatigueTimeseries sends days', async () => {
    server.use(http.get(`${BASE}/api/admin/amas/metrics/fatigue-timeseries`, ({ request }) => {
      expect(new URL(request.url).searchParams.get('days')).toBe('7');
      return ok({ points: [], avgIntensity: 0, totalTriggers: 0, threshold: 0.9 });
    }));
    expect(await adminApi.amasFatigueTimeseries()).toMatchObject({ threshold: 0.9 });
  });

  it('amasDecisionHistogram sends days', async () => {
    server.use(http.get(`${BASE}/api/admin/amas/metrics/decision-histogram`, ({ request }) => {
      expect(new URL(request.url).searchParams.get('days')).toBe('7');
      return ok({ buckets: [], p50: 0, p95: 0, totalUsers: 0 });
    }));
    expect(await adminApi.amasDecisionHistogram()).toMatchObject({ p50: 0 });
  });

  it('amasAnomaliesOverview sends days', async () => {
    server.use(http.get(`${BASE}/api/admin/amas/anomalies`, ({ request }) => {
      expect(new URL(request.url).searchParams.get('days')).toBe('14');
      return ok({ totalEvents: 0, anomalyCount: 0, violationCount: 0, coldStartExplore: 0, coldStartExploit: 0, byDay: [], topViolationFields: [] });
    }));
    expect(await adminApi.amasAnomaliesOverview(14)).toMatchObject({ totalEvents: 0 });
  });

  it('amasAnomalyFeed sends days + limit', async () => {
    server.use(http.get(`${BASE}/api/admin/amas/anomalies/feed`, ({ request }) => {
      const url = new URL(request.url);
      expect(url.searchParams.get('days')).toBe('7');
      expect(url.searchParams.get('limit')).toBe('50');
      return ok({ items: [] });
    }));
    expect(await adminApi.amasAnomalyFeed()).toMatchObject({ items: [] });
  });

  it('amasUserStateDistribution sends days + bins', async () => {
    server.use(http.get(`${BASE}/api/admin/amas/user-state/distribution`, ({ request }) => {
      const url = new URL(request.url);
      expect(url.searchParams.get('days')).toBe('2');
      expect(url.searchParams.get('bins')).toBe('30');
      return ok({ coldStartExplore: 0, coldStartExploit: 0 });
    }));
    await adminApi.amasUserStateDistribution(2, 30);
  });

  it('amasStateTransitions sends hours', async () => {
    server.use(http.get(`${BASE}/api/admin/amas/user-state/transitions`, ({ request }) => {
      expect(new URL(request.url).searchParams.get('hours')).toBe('24');
      return ok({ windowHours: 24, transitions: [] });
    }));
    expect(await adminApi.amasStateTransitions()).toMatchObject({ windowHours: 24 });
  });

  it('amasLearningClusters hits clusters endpoint', async () => {
    server.use(http.get(`${BASE}/api/admin/amas/user-state/clusters`, () => ok({ clusters: [] })));
    expect(await adminApi.amasLearningClusters()).toMatchObject({ clusters: [] });
  });

  it('amasCompareVersions sends versionA / versionB', async () => {
    server.use(http.get(`${BASE}/api/admin/amas/compare`, ({ request }) => {
      const url = new URL(request.url);
      expect(url.searchParams.get('versionA')).toBe('A');
      expect(url.searchParams.get('versionB')).toBe('B');
      return ok({ a: {}, b: {} });
    }));
    expect(await adminApi.amasCompareVersions('A', 'B')).toEqual({ a: {}, b: {} });
  });

  it('amasCompareVersionsExt sends versionA / versionB to /compare/ext', async () => {
    server.use(http.get(`${BASE}/api/admin/amas/compare/ext`, ({ request }) => {
      const url = new URL(request.url);
      expect(url.searchParams.get('versionA')).toBe('X');
      expect(url.searchParams.get('versionB')).toBe('Y');
      return ok({ a: {}, b: {} });
    }));
    expect(await adminApi.amasCompareVersionsExt('X', 'Y')).toEqual({ a: {}, b: {} });
  });
});

describe('adminApi · AMAS suggestions', () => {
  it('amasListSuggestions sends status/limit/offset, omits undefined q and status', async () => {
    server.use(http.get(`${BASE}/api/admin/amas/suggestions`, ({ request }) => {
      const url = new URL(request.url);
      expect(url.searchParams.get('status')).toBe('pending');
      expect(url.searchParams.get('limit')).toBe('20');
      expect(url.searchParams.get('offset')).toBe('5');
      expect(url.searchParams.has('q')).toBe(false);
      return ok([{ id: 1 }]);
    }));
    expect(await adminApi.amasListSuggestions('pending', 20, 5)).toEqual([{ id: 1 }]);
  });

  it('amasListSuggestions with q forwards q and default limit/offset', async () => {
    server.use(http.get(`${BASE}/api/admin/amas/suggestions`, ({ request }) => {
      const url = new URL(request.url);
      expect(url.searchParams.has('status')).toBe(false);
      expect(url.searchParams.get('limit')).toBe('50');
      expect(url.searchParams.get('offset')).toBe('0');
      expect(url.searchParams.get('q')).toBe('attn');
      return ok([]);
    }));
    expect(await adminApi.amasListSuggestions(undefined, 50, 0, 'attn')).toEqual([]);
  });

  it('amasGetSuggestion fetches by id', async () => {
    server.use(http.get(`${BASE}/api/admin/amas/suggestions/7`, () => ok({ id: 7 })));
    expect(await adminApi.amasGetSuggestion(7)).toEqual({ id: 7 });
  });

  it('amasApproveSuggestion posts note', async () => {
    let body: Record<string, unknown> = {};
    server.use(http.post(`${BASE}/api/admin/amas/suggestions/1/approve`, async ({ request }) => {
      body = await request.json() as Record<string, unknown>;
      return ok({ updated: true, versionHash: 'h', versionId: 1 });
    }));
    await adminApi.amasApproveSuggestion(1, 'lgtm');
    expect(body).toEqual({ note: 'lgtm' });
  });

  it('amasApproveSuggestion without note → empty object body', async () => {
    let body: Record<string, unknown> = { sentinel: true };
    server.use(http.post(`${BASE}/api/admin/amas/suggestions/2/approve`, async ({ request }) => {
      body = await request.json() as Record<string, unknown>;
      return ok({ updated: true, versionHash: 'h', versionId: 2 });
    }));
    await adminApi.amasApproveSuggestion(2);
    expect(body).toEqual({}); // JSON.stringify drops undefined note
  });

  it('amasRejectSuggestion posts note', async () => {
    let body: Record<string, unknown> = {};
    server.use(http.post(`${BASE}/api/admin/amas/suggestions/3/reject`, async ({ request }) => {
      body = await request.json() as Record<string, unknown>;
      return ok({ rejected: true });
    }));
    expect(await adminApi.amasRejectSuggestion(3, 'nope')).toEqual({ rejected: true });
    expect(body).toEqual({ note: 'nope' });
  });

  it('amasExplainParam posts path + currentValue', async () => {
    let body: Record<string, unknown> = {};
    server.use(http.post(`${BASE}/api/admin/amas/suggestions/explain`, async ({ request }) => {
      body = await request.json() as Record<string, unknown>;
      return ok({ explanation: 'x', model: 'm', costUsd: 0, tokensInput: 0, tokensOutput: 0 });
    }));
    await adminApi.amasExplainParam('modeling.attentionSmoothing', 0.3);
    expect(body).toEqual({ path: 'modeling.attentionSmoothing', currentValue: 0.3 });
  });

  it('amasSuggestionSpend fetches spend stats', async () => {
    server.use(http.get(`${BASE}/api/admin/amas/suggestions/spend`, () =>
      ok({ todayCostUsd: 0, todayTokensInput: 0, todayTokensOutput: 0, dailyCapUsd: 1, remainingUsd: 1 })));
    expect(await adminApi.amasSuggestionSpend()).toMatchObject({ remainingUsd: 1 });
  });

  it('amasApproveAllSuggestions posts to approve-all', async () => {
    server.use(http.post(`${BASE}/api/admin/amas/suggestions/approve-all`, () =>
      ok({ results: [{ id: 1, ok: true, error: null }] })));
    expect(await adminApi.amasApproveAllSuggestions()).toMatchObject({ results: [{ id: 1, ok: true, error: null }] });
  });

  it('amasRollbackSuggestion posts to rollback', async () => {
    server.use(http.post(`${BASE}/api/admin/amas/suggestions/9/rollback`, () =>
      ok({ rolledBack: true, versionHash: 'h' })));
    expect(await adminApi.amasRollbackSuggestion(9)).toEqual({ rolledBack: true, versionHash: 'h' });
  });

  it('amasSandboxSuggestion posts to advisor sandbox endpoint', async () => {
    server.use(http.post(`${BASE}/api/admin/amas/advisor/suggestions/4/sandbox`, () =>
      ok({ sandboxed: true })));
    expect(await adminApi.amasSandboxSuggestion(4)).toMatchObject({ sandboxed: true });
  });

  it('amasExportSuggestionsCsv fetches text/csv with status + q query (raw fetch)', async () => {
    server.use(http.get(`${BASE}/api/admin/amas/suggestions/export.csv`, ({ request }) => {
      const url = new URL(request.url);
      expect(url.searchParams.get('status')).toBe('approved');
      expect(url.searchParams.get('q')).toBe('foo');
      return new HttpResponse('id,status\n1,approved\n', { headers: { 'Content-Type': 'text/csv' } });
    }));
    const csv = await adminApi.amasExportSuggestionsCsv('approved', 'foo');
    expect(csv).toContain('id,status');
  });
});

describe('adminApi · AMAS advisor', () => {
  it('amasAdvisorCost fetches cost stats', async () => {
    server.use(http.get(`${BASE}/api/admin/amas/advisor/cost`, () => ok({ monthSpentYuan: 1 })));
    expect(await adminApi.amasAdvisorCost()).toMatchObject({ monthSpentYuan: 1 });
  });

  it('amasAdvisorCostDaily sends days', async () => {
    server.use(http.get(`${BASE}/api/admin/amas/advisor/cost/daily`, ({ request }) => {
      expect(new URL(request.url).searchParams.get('days')).toBe('30');
      return ok([{ date: '2026-06-01', costYuan: 1 }]);
    }));
    expect(await adminApi.amasAdvisorCostDaily()).toEqual([{ date: '2026-06-01', costYuan: 1 }]);
  });

  it('amasAdvisorRun posts run', async () => {
    server.use(http.post(`${BASE}/api/admin/amas/advisor/run`, () =>
      ok({ produced: true, suggestionId: 11 })));
    expect(await adminApi.amasAdvisorRun()).toEqual({ produced: true, suggestionId: 11 });
  });

  it('amasAdvisorConfig fetches config', async () => {
    server.use(http.get(`${BASE}/api/admin/amas/advisor/config`, () => ok({ advisorEnabled: true })));
    expect(await adminApi.amasAdvisorConfig()).toMatchObject({ advisorEnabled: true });
  });

  it('amasUpdateAdvisorConfig puts partial payload', async () => {
    let body: Record<string, unknown> = {};
    server.use(http.put(`${BASE}/api/admin/amas/advisor/config`, async ({ request }) => {
      body = await request.json() as Record<string, unknown>;
      return ok({ advisorEnabled: false, monthCapYuan: 50 });
    }));
    await adminApi.amasUpdateAdvisorConfig({ advisorEnabled: false, monthCapYuan: 50 } as any);
    expect(body).toEqual({ advisorEnabled: false, monthCapYuan: 50 });
  });

  it('amasListWhitelist fetches whitelist', async () => {
    server.use(http.get(`${BASE}/api/admin/amas/advisor/whitelist`, () =>
      ok([{ path: 'modeling.x', minSafe: 0, maxSafe: 1 }])));
    expect(await adminApi.amasListWhitelist()).toEqual([{ path: 'modeling.x', minSafe: 0, maxSafe: 1 }]);
  });

  it('amasAddWhitelist posts payload', async () => {
    let body: Record<string, unknown> = {};
    server.use(http.post(`${BASE}/api/admin/amas/advisor/whitelist`, async ({ request }) => {
      body = await request.json() as Record<string, unknown>;
      return ok({ path: 'modeling.y', minSafe: 0, maxSafe: 1 });
    }));
    await adminApi.amasAddWhitelist({ path: 'modeling.y', minSafe: 0, maxSafe: 1 });
    expect(body).toEqual({ path: 'modeling.y', minSafe: 0, maxSafe: 1 });
  });

  it('amasDeleteWhitelist encodes path in URL', async () => {
    server.use(http.delete(`${BASE}/api/admin/amas/advisor/whitelist/${encodeURIComponent('modeling.z')}`, () =>
      ok({ deleted: true })));
    expect(await adminApi.amasDeleteWhitelist('modeling.z')).toEqual({ deleted: true });
  });

  it('amasListCanaries fetches canary list', async () => {
    server.use(http.get(`${BASE}/api/admin/amas/advisor/canary`, () => ok([{ id: 1, percent: 10 }])));
    expect(await adminApi.amasListCanaries()).toEqual([{ id: 1, percent: 10 }]);
  });

  it('amasCreateCanary posts suggestionId + percent', async () => {
    let body: Record<string, unknown> = {};
    server.use(http.post(`${BASE}/api/admin/amas/advisor/canary`, async ({ request }) => {
      body = await request.json() as Record<string, unknown>;
      return ok({ id: 2, percent: 25 });
    }));
    await adminApi.amasCreateCanary({ suggestionId: 5, percent: 25 });
    expect(body).toEqual({ suggestionId: 5, percent: 25 });
  });

  it('amasScaleCanary posts percent to id/scale', async () => {
    let body: Record<string, unknown> = {};
    server.use(http.post(`${BASE}/api/admin/amas/advisor/canary/3/scale`, async ({ request }) => {
      body = await request.json() as Record<string, unknown>;
      return ok({ id: 3, percent: 50 });
    }));
    await adminApi.amasScaleCanary(3, 50);
    expect(body).toEqual({ percent: 50 });
  });

  it('amasRollbackCanary posts to id/rollback', async () => {
    server.use(http.post(`${BASE}/api/admin/amas/advisor/canary/4/rollback`, () =>
      ok({ rolledBack: true })));
    expect(await adminApi.amasRollbackCanary(4)).toEqual({ rolledBack: true });
  });

  it('amasPromoteCanary posts to id/promote', async () => {
    server.use(http.post(`${BASE}/api/admin/amas/advisor/canary/5/promote`, () =>
      ok({ promoted: true, versionHash: 'h5' })));
    expect(await adminApi.amasPromoteCanary(5)).toEqual({ promoted: true, versionHash: 'h5' });
  });
});

describe('adminApi · wordbook-center', () => {
  it('wbCenterBrowse fetches browse list', async () => {
    server.use(http.get(`${BASE}/api/admin/wordbook-center/browse`, () => ok([{ id: 'wb-1' }])));
    expect(await adminApi.wbCenterBrowse()).toEqual([{ id: 'wb-1' }]);
  });

  it('wbCenterPreview forwards page/perPage and path id', async () => {
    server.use(http.get(`${BASE}/api/admin/wordbook-center/browse/wb-9`, ({ request }) => {
      const url = new URL(request.url);
      expect(url.searchParams.get('page')).toBe('2');
      expect(url.searchParams.get('perPage')).toBe('50');
      return ok({ id: 'wb-9', title: 't', words: [], total: 0 });
    }));
    expect(await adminApi.wbCenterPreview('wb-9', { page: 2, perPage: 50 })).toMatchObject({ id: 'wb-9' });
  });

  it('wbCenterImport posts to import/:id', async () => {
    server.use(http.post(`${BASE}/api/admin/wordbook-center/import/wb-1`, () =>
      ok({ imported: 3, skipped: 0, errors: [] })));
    expect(await adminApi.wbCenterImport('wb-1')).toMatchObject({ imported: 3 });
  });

  it('wbCenterUpdates fetches updates list', async () => {
    server.use(http.get(`${BASE}/api/admin/wordbook-center/updates`, () => ok([{ id: 'wb-1' }])));
    expect(await adminApi.wbCenterUpdates()).toEqual([{ id: 'wb-1' }]);
  });

  it('wbCenterSync posts to updates/:id/sync', async () => {
    server.use(http.post(`${BASE}/api/admin/wordbook-center/updates/wb-2/sync`, () =>
      ok({ updated: 1, added: 0, removed: 0 })));
    expect(await adminApi.wbCenterSync('wb-2')).toMatchObject({ updated: 1 });
  });

  it('wbCenterUpload posts full payload', async () => {
    let body: Record<string, unknown> = {};
    server.use(http.post(`${BASE}/api/admin/wordbook-center/upload`, async ({ request }) => {
      body = await request.json() as Record<string, unknown>;
      return ok({ imported: 2, skipped: 0, errors: [] });
    }));
    const payload = {
      id: 'wb-up', name: 'Up', version: '1.0', tags: ['cet4'],
      words: [{ spelling: 'apple', meanings: ['苹果'] }],
    };
    await adminApi.wbCenterUpload(payload);
    expect(body).toEqual(payload);
  });

  it('wbCenterPatchTags patches tag ops to /:id/tags', async () => {
    let body: Record<string, unknown> = {};
    server.use(http.patch(`${BASE}/api/admin/wordbook-center/wb-3/tags`, async ({ request }) => {
      body = await request.json() as Record<string, unknown>;
      return ok({ tags: ['a', 'b'] });
    }));
    expect(await adminApi.wbCenterPatchTags('wb-3', { add: ['a'], remove: ['x'] })).toEqual({ tags: ['a', 'b'] });
    expect(body).toEqual({ add: ['a'], remove: ['x'] });
  });
});

describe('adminApi · admin wordbooks CRUD', () => {
  it('adminWordbooksList forwards query params', async () => {
    server.use(http.get(`${BASE}/api/admin/wordbooks`, ({ request }) => {
      const url = new URL(request.url);
      expect(url.searchParams.get('page')).toBe('1');
      expect(url.searchParams.get('q')).toBe('cet');
      return ok({ data: [], total: 0, page: 1, perPage: 20, totalPages: 0 });
    }));
    await adminApi.adminWordbooksList({ page: 1, q: 'cet' } as any);
  });

  it('adminWordbookStats fetches /:id/stats', async () => {
    server.use(http.get(`${BASE}/api/admin/wordbooks/wb-1/stats`, () => ok({ wordCount: 100 })));
    expect(await adminApi.adminWordbookStats('wb-1')).toMatchObject({ wordCount: 100 });
  });

  it('adminWordbookWords forwards query to /:id/words', async () => {
    server.use(http.get(`${BASE}/api/admin/wordbooks/wb-1/words`, ({ request }) => {
      expect(new URL(request.url).searchParams.get('page')).toBe('3');
      return ok({ data: [], total: 0, page: 3, perPage: 20, totalPages: 0 });
    }));
    await adminApi.adminWordbookWords('wb-1', { page: 3 } as any);
  });

  it('adminWordbookHeatmap sends default limit 600', async () => {
    server.use(http.get(`${BASE}/api/admin/wordbooks/wb-1/heatmap`, ({ request }) => {
      expect(new URL(request.url).searchParams.get('limit')).toBe('600');
      return ok({ cells: [] });
    }));
    await adminApi.adminWordbookHeatmap('wb-1');
  });

  it('adminWordbookDistribution fetches /:id/user-distribution', async () => {
    server.use(http.get(`${BASE}/api/admin/wordbooks/wb-1/user-distribution`, () => ok({ buckets: [] })));
    expect(await adminApi.adminWordbookDistribution('wb-1')).toMatchObject({ buckets: [] });
  });

  it('adminWordbookHistory forwards page/perPage to /:id/history', async () => {
    server.use(http.get(`${BASE}/api/admin/wordbooks/wb-1/history`, ({ request }) => {
      const url = new URL(request.url);
      expect(url.searchParams.get('page')).toBe('1');
      expect(url.searchParams.get('perPage')).toBe('10');
      return ok({ data: [], total: 0, page: 1, perPage: 10, totalPages: 0 });
    }));
    await adminApi.adminWordbookHistory('wb-1', { page: 1, perPage: 10 });
  });

  it('adminWordbookCreate posts payload', async () => {
    let body: Record<string, unknown> = {};
    server.use(http.post(`${BASE}/api/admin/wordbooks`, async ({ request }) => {
      body = await request.json() as Record<string, unknown>;
      return ok({ id: 'wb-new', name: 'New' });
    }));
    await adminApi.adminWordbookCreate({ name: 'New' } as any);
    expect(body).toEqual({ name: 'New' });
  });

  it('adminWordbookUpdate patches /:id', async () => {
    let body: Record<string, unknown> = {};
    server.use(http.patch(`${BASE}/api/admin/wordbooks/wb-1`, async ({ request }) => {
      body = await request.json() as Record<string, unknown>;
      return ok({ id: 'wb-1', name: 'Renamed' });
    }));
    await adminApi.adminWordbookUpdate('wb-1', { name: 'Renamed' } as any);
    expect(body).toEqual({ name: 'Renamed' });
  });

  it('adminWordbookDelete deletes /:id', async () => {
    server.use(http.delete(`${BASE}/api/admin/wordbooks/wb-1`, () => ok({ deleted: true })));
    expect(await adminApi.adminWordbookDelete('wb-1')).toEqual({ deleted: true });
  });

  it('adminWordbookAddWord posts to /:id/words', async () => {
    let body: Record<string, unknown> = {};
    server.use(http.post(`${BASE}/api/admin/wordbooks/wb-1/words`, async ({ request }) => {
      body = await request.json() as Record<string, unknown>;
      return ok({ id: 'w-1', spelling: 'cat' });
    }));
    await adminApi.adminWordbookAddWord('wb-1', { spelling: 'cat' } as any);
    expect(body).toEqual({ spelling: 'cat' });
  });

  it('adminWordbookRemoveWord deletes /:id/words/:wordId', async () => {
    server.use(http.delete(`${BASE}/api/admin/wordbooks/wb-1/words/w-1`, () => ok({ removed: true })));
    expect(await adminApi.adminWordbookRemoveWord('wb-1', 'w-1')).toEqual({ removed: true });
  });

  it('adminWordbookExport fetches /:id/export', async () => {
    server.use(http.get(`${BASE}/api/admin/wordbooks/wb-1/export`, () => ok({ name: 'X', words: [] })));
    expect(await adminApi.adminWordbookExport('wb-1')).toMatchObject({ name: 'X' });
  });
});

describe('adminApi · clients / devices', () => {
  it('getClients returns sseLive + recentlyActive', async () => {
    const payload = { sseLive: [], recentlyActive: [] };
    server.use(http.get(`${BASE}/api/admin/clients`, () => ok(payload)));
    expect(await adminApi.getClients()).toEqual(payload);
  });

  it('banClient sends { reason } when provided, omits body otherwise', async () => {
    const bodies: string[] = [];
    server.use(http.post(`${BASE}/api/admin/clients/d-1/ban`, async ({ request }) => {
      bodies.push(await request.text());
      return ok({ banned: true, deviceId: 'd-1' });
    }));
    await adminApi.banClient('d-1', 'spam');
    await adminApi.banClient('d-1');
    expect(bodies[0]).toBe(JSON.stringify({ reason: 'spam' }));
    expect(bodies[1]).toBe('');
  });

  it('unbanClient posts to /:id/unban', async () => {
    server.use(http.post(`${BASE}/api/admin/clients/d-9/unban`, () =>
      ok({ banned: false, deviceId: 'd-9' })));
    expect(await adminApi.unbanClient('d-9')).toEqual({ banned: false, deviceId: 'd-9' });
  });

  it('requestTelemetry posts to /:id/request-telemetry', async () => {
    server.use(http.post(`${BASE}/api/admin/clients/d-2/request-telemetry`, () =>
      ok({ requestId: 'req-1' })));
    expect(await adminApi.requestTelemetry('d-2')).toEqual({ requestId: 'req-1' });
  });

  it('getTelemetry forwards limit/offset/eventType query', async () => {
    server.use(http.get(`${BASE}/api/admin/telemetry/d-3`, ({ request }) => {
      const url = new URL(request.url);
      expect(url.searchParams.get('limit')).toBe('50');
      expect(url.searchParams.get('offset')).toBe('100');
      expect(url.searchParams.get('eventType')).toBe('session');
      return ok({ records: [], total: 0 });
    }));
    await adminApi.getTelemetry('d-3', { limit: 50, offset: 100, eventType: 'session' });
  });

  it('getTelemetrySummary fetches /:id/summary', async () => {
    server.use(http.get(`${BASE}/api/admin/telemetry/d-4/summary`, () =>
      ok({ total: 0, firstTs: null, lastTs: null, byEventType: [], deviceProfile: null, featureUsage: [], routes: [], clickTargets: [], totalClicks: 0, totalErrors: 0, totalDurationSecs: 0, sessionCount: 0 })));
    expect(await adminApi.getTelemetrySummary('d-4')).toMatchObject({ total: 0 });
  });

  it('getClientDetail fetches /clients/:id', async () => {
    server.use(http.get(`${BASE}/api/admin/clients/d-5`, () =>
      ok({ deviceId: 'd-5', platform: 'web', online: true, connectionCount: 1 })));
    expect(await adminApi.getClientDetail('d-5')).toMatchObject({ deviceId: 'd-5', online: true });
  });

  it('getClientsPaginated forwards page/perPage/q/platform/recentMinutes', async () => {
    server.use(http.get(`${BASE}/api/admin/clients/paginated`, ({ request }) => {
      const url = new URL(request.url);
      expect(url.searchParams.get('page')).toBe('2');
      expect(url.searchParams.get('perPage')).toBe('30');
      expect(url.searchParams.get('q')).toBe('foo');
      expect(url.searchParams.get('platform')).toBe('ios');
      expect(url.searchParams.get('recentMinutes')).toBe('60');
      return ok({ data: [], total: 0, page: 2, perPage: 30, totalPages: 0 });
    }));
    await adminApi.getClientsPaginated({ page: 2, perPage: 30, q: 'foo', platform: 'ios', recentMinutes: 60 });
  });

  it('getClientsDistribution fetches /clients/distribution', async () => {
    server.use(http.get(`${BASE}/api/admin/clients/distribution`, () =>
      ok({ platforms: [], versions: [], policies: [] })));
    expect(await adminApi.getClientsDistribution()).toMatchObject({ platforms: [] });
  });

  it('putUpgradePolicy puts payload to /upgrade-policy/:platform', async () => {
    let body: Record<string, unknown> = {};
    server.use(http.put(`${BASE}/api/admin/clients/upgrade-policy/web`, async ({ request }) => {
      body = await request.json() as Record<string, unknown>;
      return ok({ ok: true, platform: 'web' });
    }));
    await adminApi.putUpgradePolicy('web', { minVersion: '1.0.0', grayscalePct: 20, pwaSilentUpdate: true });
    expect(body).toEqual({ minVersion: '1.0.0', grayscalePct: 20, pwaSilentUpdate: true });
  });

  it('broadcastUpgrade posts payload to /broadcast-upgrade/:platform', async () => {
    let body: Record<string, unknown> = {};
    server.use(http.post(`${BASE}/api/admin/clients/broadcast-upgrade/android`, async ({ request }) => {
      body = await request.json() as Record<string, unknown>;
      return ok({ matched: 3, pushedConnections: 2 });
    }));
    const res = await adminApi.broadcastUpgrade('android', { belowVersion: '1.0.0', latestVersion: '1.1.0', message: 'up' });
    expect(res).toEqual({ matched: 3, pushedConnections: 2 });
    expect(body).toEqual({ belowVersion: '1.0.0', latestVersion: '1.1.0', message: 'up' });
  });
});
