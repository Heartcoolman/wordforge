import { describe, it, expect, vi, beforeAll, afterAll, afterEach } from 'vitest';
import { setupServer } from 'msw/node';
import { http, HttpResponse } from 'msw';

const BASE = 'http://localhost:3000';

vi.mock('@/lib/token', () => ({
  tokenManager: {
    getToken: () => null,
    getAdminToken: () => null,
    setTokens: vi.fn(),
    clearTokens: vi.fn(),
    needsRefresh: () => false,
    isAuthenticated: () => false,
    setAdminToken: vi.fn(),
  },
}));
vi.mock('@/api/auth', () => ({ authApi: { refresh: vi.fn() } }));

import { recordsApi } from '@/api/records';

const server = setupServer();
beforeAll(() => server.listen({ onUnhandledRequest: 'bypass' }));
afterEach(() => server.resetHandlers());
afterAll(() => server.close());

describe('recordsApi', () => {
  it('list fetches records with params', async () => {
    const records = [{ id: 'r1', wordId: 'w1', isCorrect: true, responseTimeMs: 500, createdAt: '2025-01-01' }];
    server.use(
      http.get(`${BASE}/api/records`, () =>
        HttpResponse.json({ success: true, data: records })),
    );
    const result = await recordsApi.list({ limit: 10, offset: 0 });
    expect(result).toEqual(records);
  });

  it('create posts a new record', async () => {
    const record = { id: 'r2', wordId: 'w1', isCorrect: true, responseTimeMs: 300 };
    server.use(
      http.post(`${BASE}/api/records`, () =>
        HttpResponse.json({ success: true, data: record })),
    );
    const result = await recordsApi.create({ wordId: 'w1', isCorrect: true, responseTimeMs: 300 });
    expect(result).toEqual(record);
  });

  it('batchCreate posts multiple records', async () => {
    const data = { count: 2, items: [{ id: 'r3' }, { id: 'r4' }] };
    server.use(
      http.post(`${BASE}/api/records/batch`, () =>
        HttpResponse.json({ success: true, data })),
    );
    const result = await recordsApi.batchCreate([
      { wordId: 'w1', isCorrect: true, responseTimeMs: 100 },
      { wordId: 'w2', isCorrect: false, responseTimeMs: 200 },
    ]);
    expect(result).toEqual(data);
  });

  it('statistics fetches record statistics', async () => {
    const stats = { totalRecords: 100, correctRate: 0.85 };
    server.use(
      http.get(`${BASE}/api/records/statistics`, () =>
        HttpResponse.json({ success: true, data: stats })),
    );
    const result = await recordsApi.statistics();
    expect(result).toEqual(stats);
  });

  it('enhancedStatistics fetches enhanced stats', async () => {
    const enhanced = { daily: [{ date: '2025-01-01', total: 10, correct: 8, accuracy: 0.8 }] };
    server.use(
      http.get(`${BASE}/api/records/statistics/enhanced`, () =>
        HttpResponse.json({ success: true, data: enhanced })),
    );
    const result = await recordsApi.enhancedStatistics();
    expect(result).toEqual(enhanced);
  });
});
