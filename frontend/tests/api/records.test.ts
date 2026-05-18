import { describe, it, expect, vi, beforeAll, afterAll, afterEach } from 'vitest';
import { setupServer } from 'msw/node';
import { http, HttpResponse } from 'msw';

import { TEST_BASE_URL as BASE } from '../helpers/constants';

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
    const paginatedResponse = { data: records, total: 1, page: 1, perPage: 10, totalPages: 1 };
    server.use(
      http.get(`${BASE}/api/records`, () =>
        HttpResponse.json({ success: true, data: paginatedResponse })),
    );
    const result = await recordsApi.list({ page: 1, perPage: 10 });
    expect(result).toEqual(paginatedResponse);
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

  it('create rounds every optional metric when provided', async () => {
    let body: Record<string, unknown> = {};
    server.use(
      http.post(`${BASE}/api/records`, async ({ request }) => {
        body = await request.json() as Record<string, unknown>;
        return HttpResponse.json({ success: true, data: { id: 'r5' } });
      }),
    );
    await recordsApi.create({
      wordId: 'w1',
      isCorrect: false,
      responseTimeMs: 123.7,
      dwellTimeMs: 2000.4,
      pauseCount: 1.6,
      switchCount: 2.4,
      retryCount: 3.5,
      focusLossDurationMs: 400.9,
      pausedTimeMs: 88.3,
    } as any);
    expect(body).toMatchObject({
      wordId: 'w1',
      isCorrect: false,
      responseTimeMs: 124,
      dwellTimeMs: 2000,
      pauseCount: 2,
      switchCount: 2,
      retryCount: 4,
      focusLossDurationMs: 401,
      pausedTimeMs: 88,
    });
  });

  it('batchCreate sanitizes every record in payload', async () => {
    let body: { records: Array<Record<string, unknown>> } = { records: [] };
    server.use(
      http.post(`${BASE}/api/records/batch`, async ({ request }) => {
        body = await request.json() as { records: Array<Record<string, unknown>> };
        return HttpResponse.json({ success: true, data: { count: 2, failed: 0, partial: false, items: [], errors: [] } });
      }),
    );
    await recordsApi.batchCreate([
      { wordId: 'a', isCorrect: true, responseTimeMs: 10.6 },
      { wordId: 'b', isCorrect: false, responseTimeMs: 20.4, pauseCount: 1.7 } as any,
    ]);
    expect(body.records[0]?.responseTimeMs).toBe(11);
    expect(body.records[1]?.pauseCount).toBe(2);
  });
});
