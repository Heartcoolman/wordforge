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

import { amasApi } from '@/api/amas';

describe('amasApi', () => {
  it('getState returns user state', async () => {
    const state = { userId: 'u1', level: 3, phase: 'Exploit' };
    server.use(
      http.get(`${BASE}/api/amas/state`, () =>
        HttpResponse.json({ success: true, data: state })),
    );
    const result = await amasApi.getState();
    expect(result).toEqual(state);
  });

  it('getStrategy returns strategy', async () => {
    const strategy = { type: 'spaced-repetition', params: { interval: 1.5 } };
    server.use(
      http.get(`${BASE}/api/amas/strategy`, () =>
        HttpResponse.json({ success: true, data: strategy })),
    );
    const result = await amasApi.getStrategy();
    expect(result).toEqual(strategy);
  });

  it('getPhase returns current phase', async () => {
    server.use(
      http.get(`${BASE}/api/amas/phase`, () =>
        HttpResponse.json({ success: true, data: { phase: 'Classify' } })),
    );
    const result = await amasApi.getPhase();
    expect(result).toEqual({ phase: 'Classify' });
  });

  it('getLearningCurve returns curve data', async () => {
    const curve = [{ day: 1, accuracy: 0.6 }, { day: 2, accuracy: 0.75 }];
    server.use(
      http.get(`${BASE}/api/amas/learning-curve`, () =>
        HttpResponse.json({ success: true, data: { curve } })),
    );
    const result = await amasApi.getLearningCurve();
    expect(result).toEqual({ curve });
  });

  it('getIntervention returns interventions list', async () => {
    const interventions = [{ type: 'hint', message: 'Try again' }];
    server.use(
      http.get(`${BASE}/api/amas/intervention`, () =>
        HttpResponse.json({ success: true, data: { interventions } })),
    );
    const result = await amasApi.getIntervention();
    expect(result).toEqual({ interventions });
  });

  it('reset posts and returns reset status', async () => {
    server.use(
      http.post(`${BASE}/api/amas/reset`, () =>
        HttpResponse.json({ success: true, data: { reset: true } })),
    );
    const result = await amasApi.reset();
    expect(result).toEqual({ reset: true });
  });

  it('evaluateMastery sends wordId as query param', async () => {
    const evaluation = {
      wordId: 'w1',
      state: 'learning',
      masteryLevel: 0.6,
      correctStreak: 3,
      totalAttempts: 10,
      nextReviewDate: '2026-02-15',
    };
    server.use(
      http.get(`${BASE}/api/amas/mastery/evaluate`, ({ request }) => {
        const url = new URL(request.url);
        expect(url.searchParams.get('wordId')).toBe('w1');
        return HttpResponse.json({ success: true, data: evaluation });
      }),
    );
    const result = await amasApi.evaluateMastery('w1');
    expect(result).toEqual(evaluation);
  });

  it('getConfig returns AMAS config (admin)', async () => {
    const config = { learningRate: 0.01, batchSize: 32 };
    server.use(
      http.get(`${BASE}/api/amas/config`, () =>
        HttpResponse.json({ success: true, data: config })),
    );
    const result = await amasApi.getConfig();
    expect(result).toEqual(config);
  });

  it('updateConfig sends config and returns updated status', async () => {
    const config = { learningRate: 0.02, batchSize: 64 };
    server.use(
      http.put(`${BASE}/api/amas/config`, async ({ request }) => {
        const body = await request.json() as Record<string, unknown>;
        expect(body).toEqual(config);
        return HttpResponse.json({ success: true, data: { updated: true } });
      }),
    );
    const result = await amasApi.updateConfig(config);
    expect(result).toEqual({ updated: true });
  });

  it('getMetrics returns AMAS metrics (admin)', async () => {
    const metrics = { totalUsers: 200, activeSessions: 15, avgAccuracy: 0.78, avgResponseTime: 1.2 };
    server.use(
      http.get(`${BASE}/api/amas/metrics`, () =>
        HttpResponse.json({ success: true, data: metrics })),
    );
    const result = await amasApi.getMetrics();
    expect(result).toEqual(metrics);
  });

  it('getMonitoring sends limit as query param and returns events', async () => {
    const events = [{ timestamp: '2026-02-10T00:00:00Z', eventType: 'session_start', data: {} }];
    server.use(
      http.get(`${BASE}/api/amas/monitoring`, ({ request }) => {
        const url = new URL(request.url);
        expect(url.searchParams.get('limit')).toBe('25');
        return HttpResponse.json({ success: true, data: events });
      }),
    );
    const result = await amasApi.getMonitoring(25);
    expect(result).toEqual(events);
  });

  it('getMonitoring uses default limit of 50', async () => {
    const events: any[] = [];
    server.use(
      http.get(`${BASE}/api/amas/monitoring`, ({ request }) => {
        const url = new URL(request.url);
        expect(url.searchParams.get('limit')).toBe('50');
        return HttpResponse.json({ success: true, data: events });
      }),
    );
    const result = await amasApi.getMonitoring();
    expect(result).toEqual(events);
  });
});
