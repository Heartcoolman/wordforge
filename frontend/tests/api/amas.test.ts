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

import { amasApi } from '@/api/amas';

describe('amasApi', () => {
  it('processEvent sends payload and returns process result', async () => {
    const result = {
      sessionId: 's-1',
      strategy: { difficulty: 0.5, batchSize: 10, newRatio: 0.3, intervalScale: 1, reviewMode: false },
      explanation: { primaryReason: 'ok', factors: [] },
      state: { attention: 0.7, fatigue: 0.2, motivation: 0.3, confidence: 0.5, sessionEventCount: 1, totalEventCount: 1, createdAt: '2026-02-14T00:00:00Z' },
      reward: { value: 0.8, components: { accuracyReward: 1, speedReward: 1, fatiguePenalty: 0, frustrationPenalty: 0 } },
    };
    server.use(
      http.post(`${BASE}/api/amas/process-event`, async ({ request }) => {
        const body = await request.json() as Record<string, unknown>;
        expect(body).toMatchObject({
          wordId: 'word-1',
          isCorrect: true,
          responseTime: 901,
          hintUsed: true,
        });
        return HttpResponse.json({ success: true, data: result });
      }),
    );
    const response = await amasApi.processEvent({
      wordId: 'word-1',
      isCorrect: true,
      responseTime: 900.6,
      hintUsed: true,
    });
    expect(response).toEqual(result);
  });

  it('batchProcess sends events and returns batch result', async () => {
    const result = {
      count: 2,
      items: [{ sessionId: 's-2' }, { sessionId: 's-3' }],
    };
    server.use(
      http.post(`${BASE}/api/amas/batch-process`, async ({ request }) => {
        const body = await request.json() as { events: Array<Record<string, unknown>> };
        expect(body.events).toHaveLength(2);
        expect(body.events[0]?.responseTime).toBe(801);
        expect(body.events[1]?.responseTime).toBe(902);
        return HttpResponse.json({ success: true, data: result });
      }),
    );
    const response = await amasApi.batchProcess([
      { wordId: 'w-1', isCorrect: true, responseTime: 800.8 },
      { wordId: 'w-2', isCorrect: false, responseTime: 901.9 },
    ]);
    expect(response).toEqual(result);
  });

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
      http.get(`${BASE}/api/admin/amas/config`, () =>
        HttpResponse.json({ success: true, data: config })),
    );
    const result = await amasApi.getConfig();
    expect(result).toEqual(config);
  });

  it('updateConfig sends config and returns updated status', async () => {
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
      http.put(`${BASE}/api/admin/amas/config`, async ({ request }) => {
        const body = await request.json() as Record<string, unknown>;
        expect(body).toEqual(config);
        return HttpResponse.json({ success: true, data: { updated: true } });
      }),
    );
    const result = await amasApi.updateConfig(config);
    expect(result).toEqual({ updated: true });
  });

  it('getMetrics returns AMAS metrics (admin)', async () => {
    const metrics = { heuristic: { callCount: 10, totalLatencyUs: 500, errorCount: 0 }, ige: { callCount: 5, totalLatencyUs: 300, errorCount: 1 } };
    server.use(
      http.get(`${BASE}/api/admin/amas/metrics`, () =>
        HttpResponse.json({ success: true, data: metrics })),
    );
    const result = await amasApi.getMetrics();
    expect(result).toEqual(metrics);
  });

  it('getMonitoring sends limit as query param and returns events', async () => {
    const events = [{ timestamp: '2026-02-10T00:00:00Z', eventType: 'session_start', data: {} }];
    server.use(
      http.get(`${BASE}/api/admin/amas/monitoring`, ({ request }) => {
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
      http.get(`${BASE}/api/admin/amas/monitoring`, ({ request }) => {
        const url = new URL(request.url);
        expect(url.searchParams.get('limit')).toBe('50');
        return HttpResponse.json({ success: true, data: events });
      }),
    );
    const result = await amasApi.getMonitoring();
    expect(result).toEqual(events);
  });
});
