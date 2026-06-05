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
    refreshAccessToken: vi.fn(),
    isAuthenticated: () => false,
    setAdminToken: vi.fn(),
  },
}));
vi.mock('@/api/auth', () => ({ authApi: { refresh: vi.fn() } }));

const ok = (data: unknown) => ({ success: true, data });

// ═══════════════════════ packs.ts ═══════════════════════
describe('packsApi (packs.ts)', () => {
  it('list GET /api/admin/resource-packs returns entries', async () => {
    const { packsApi } = await import('@/api/packs');
    const entry = { packId: 'p1', description: null, createdAt: 't', updatedAt: 't', versions: [], active: { stable: null, beta: null, internal: null }, totalInstalls: 0, outcomes7d: { installed: 0, verify_failed: 0, rollback: 0 } };
    server.use(http.get(`${BASE}/api/admin/resource-packs`, () => HttpResponse.json(ok([entry]))));
    const res = await packsApi.list();
    expect(res).toEqual([entry]);
  });

  it('summary GET /summary returns KPI aggregate', async () => {
    const { packsApi } = await import('@/api/packs');
    const summary = { totalPacks: 2, newPacksThisMonth: 1, totalVersions: 5, versionsByChannel: { stable: 3, beta: 1, internal: 1 }, installsToday: 10, installsTodaySuccess: 9, failureRate7d: 0.1, failures7dByOutcome: { verify_failed: 1, rollback: 0 }, onlineClients: 3 };
    server.use(http.get(`${BASE}/api/admin/resource-packs/summary`, () => HttpResponse.json(ok(summary))));
    const res = await packsApi.summary();
    expect(res.totalPacks).toBe(2);
    expect(res.onlineClients).toBe(3);
  });

  it('setActive PUT channel/active sends {version} and returns activated', async () => {
    const { packsApi } = await import('@/api/packs');
    let body: any = null;
    let hitUrl = '';
    server.use(http.put(`${BASE}/api/admin/resource-packs/:packId/channel/:channel/active`, async ({ request }) => {
      hitUrl = request.url;
      body = await request.json();
      return HttpResponse.json(ok({ packId: 'p 1', channel: 'beta', version: '1.2.0', activated: true, audienceClients: 7 }));
    }));
    const res = await packsApi.setActive('p 1', 'beta', '1.2.0');
    expect(body).toEqual({ version: '1.2.0' });
    expect(hitUrl).toContain('p%201'); // encodeURIComponent(packId)
    expect(hitUrl).toContain('/channel/beta/active');
    expect(res.activated).toBe(true);
    expect(res.audienceClients).toBe(7);
  });

  it('deactivateVersion DELETE versions/:version returns deactivated', async () => {
    const { packsApi } = await import('@/api/packs');
    let hitUrl = '';
    server.use(http.delete(`${BASE}/api/admin/resource-packs/:packId/versions/:version`, ({ request }) => {
      hitUrl = request.url;
      return HttpResponse.json(ok({ deactivated: true }));
    }));
    const res = await packsApi.deactivateVersion('pack/a', '2.0.0-rc.1');
    expect(hitUrl).toContain('pack%2Fa');
    expect(hitUrl).toContain('2.0.0-rc.1');
    expect(res.deactivated).toBe(true);
  });

  it('stats GET /:packId/stats returns wrapped stats', async () => {
    const { packsApi } = await import('@/api/packs');
    server.use(http.get(`${BASE}/api/admin/resource-packs/:packId/stats`, () =>
      HttpResponse.json(ok({ packId: 'p1', stats: [{ version: '1.0.0', outcome: 'installed', count: 5 }] }))));
    const res = await packsApi.stats('p1');
    expect(res.packId).toBe('p1');
    expect(res.stats[0].count).toBe(5);
  });

    });

// ═══════════════════════ probeTelemetry.ts ═══════════════════════
describe('probeTelemetryApi (probeTelemetry.ts)', () => {
  it('overview GET /overview forwards days', async () => {
    const { probeTelemetryApi } = await import('@/api/probeTelemetry');
    let url = '';
    server.use(http.get(`${BASE}/api/admin/probe-telemetry/overview`, ({ request }) => {
      url = request.url;
      return HttpResponse.json(ok({ total: 1 }));
    }));
    const res = await probeTelemetryApi.overview(7);
    expect(url).toContain('days=7');
    expect(res).toEqual({ total: 1 });
  });

  it('probes GET /probes forwards days', async () => {
    const { probeTelemetryApi } = await import('@/api/probeTelemetry');
    server.use(http.get(`${BASE}/api/admin/probe-telemetry/probes`, ({ request }) => {
      expect(new URL(request.url).searchParams.get('days')).toBe('30');
      return HttpResponse.json(ok({ probes: [] }));
    }));
    const res = await probeTelemetryApi.probes(30);
    expect(res).toEqual({ probes: [] });
  });

  it('sampling GET /sampling', async () => {
    const { probeTelemetryApi } = await import('@/api/probeTelemetry');
    server.use(http.get(`${BASE}/api/admin/probe-telemetry/sampling`, () => HttpResponse.json(ok({ rules: [] }))));
    expect(await probeTelemetryApi.sampling()).toEqual({ rules: [] });
  });

  it('updateSamplingRule PATCH encodes eventType + sends body', async () => {
    const { probeTelemetryApi } = await import('@/api/probeTelemetry');
    let body: any = null;
    let url = '';
    server.use(http.patch(`${BASE}/api/admin/probe-telemetry/sampling/:eventType`, async ({ request }) => {
      url = request.url;
      body = await request.json();
      return HttpResponse.json(ok({ updated: true }));
    }));
    await probeTelemetryApi.updateSamplingRule('amas/event', { sampleRate: 0.5, enabled: true });
    expect(url).toContain('amas%2Fevent');
    expect(body).toEqual({ sampleRate: 0.5, enabled: true });
  });

  it('sinks GET /sinks', async () => {
    const { probeTelemetryApi } = await import('@/api/probeTelemetry');
    server.use(http.get(`${BASE}/api/admin/probe-telemetry/sinks`, () => HttpResponse.json(ok({ sinks: [] }))));
    expect(await probeTelemetryApi.sinks()).toEqual({ sinks: [] });
  });

  it('schema GET /schema forwards eventType', async () => {
    const { probeTelemetryApi } = await import('@/api/probeTelemetry');
    server.use(http.get(`${BASE}/api/admin/probe-telemetry/schema`, ({ request }) => {
      expect(new URL(request.url).searchParams.get('eventType')).toBe('learning');
      return HttpResponse.json(ok({ fields: [] }));
    }));
    expect(await probeTelemetryApi.schema('learning')).toEqual({ fields: [] });
  });

  it('audit GET /audit default limit 20 + custom', async () => {
    const { probeTelemetryApi } = await import('@/api/probeTelemetry');
    let limit = '';
    server.use(http.get(`${BASE}/api/admin/probe-telemetry/audit`, ({ request }) => {
      limit = new URL(request.url).searchParams.get('limit') ?? '';
      return HttpResponse.json(ok({ entries: [] }));
    }));
    await probeTelemetryApi.audit();
    expect(limit).toBe('20');
    await probeTelemetryApi.audit(50);
    expect(limit).toBe('50');
  });

  it('stream GET /stream default limit 30', async () => {
    const { probeTelemetryApi } = await import('@/api/probeTelemetry');
    server.use(http.get(`${BASE}/api/admin/probe-telemetry/stream`, ({ request }) => {
      expect(new URL(request.url).searchParams.get('limit')).toBe('30');
      return HttpResponse.json(ok({ events: [] }));
    }));
    expect(await probeTelemetryApi.stream()).toEqual({ events: [] });
  });
});
