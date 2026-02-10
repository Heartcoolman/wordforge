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

import { wordStatesApi } from '@/api/wordStates';

describe('wordStatesApi', () => {
  it('get returns learning state for a word', async () => {
    const state = { wordId: 'w1', level: 3, nextReview: '2026-02-15' };
    server.use(
      http.get(`${BASE}/api/word-states/w1`, () =>
        HttpResponse.json({ success: true, data: state })),
    );
    const result = await wordStatesApi.get('w1');
    expect(result).toEqual(state);
  });

  it('batchGet sends wordIds and returns states', async () => {
    const states = [
      { wordId: 'w1', level: 3 },
      { wordId: 'w2', level: 1 },
    ];
    server.use(
      http.post(`${BASE}/api/word-states/batch`, async ({ request }) => {
        const body = await request.json() as Record<string, unknown>;
        expect(body).toEqual({ wordIds: ['w1', 'w2'] });
        return HttpResponse.json({ success: true, data: states });
      }),
    );
    const result = await wordStatesApi.batchGet(['w1', 'w2']);
    expect(result).toEqual(states);
  });

  it('getDueList sends limit as query param', async () => {
    const dueList = [{ wordId: 'w1', level: 2, nextReview: '2026-02-10' }];
    server.use(
      http.get(`${BASE}/api/word-states/due/list`, ({ request }) => {
        const url = new URL(request.url);
        expect(url.searchParams.get('limit')).toBe('20');
        return HttpResponse.json({ success: true, data: dueList });
      }),
    );
    const result = await wordStatesApi.getDueList(20);
    expect(result).toEqual(dueList);
  });

  it('getDueList uses default limit of 50', async () => {
    server.use(
      http.get(`${BASE}/api/word-states/due/list`, ({ request }) => {
        const url = new URL(request.url);
        expect(url.searchParams.get('limit')).toBe('50');
        return HttpResponse.json({ success: true, data: [] });
      }),
    );
    const result = await wordStatesApi.getDueList();
    expect(result).toEqual([]);
  });

  it('getOverview returns word state overview', async () => {
    const overview = { total: 500, mastered: 200, learning: 150, new: 150 };
    server.use(
      http.get(`${BASE}/api/word-states/stats/overview`, () =>
        HttpResponse.json({ success: true, data: overview })),
    );
    const result = await wordStatesApi.getOverview();
    expect(result).toEqual(overview);
  });

  it('batchUpdate sends update data and returns count', async () => {
    const updateData = { updates: [{ wordId: 'w1', correct: true }] };
    server.use(
      http.post(`${BASE}/api/word-states/batch-update`, async ({ request }) => {
        const body = await request.json() as Record<string, unknown>;
        expect(body).toEqual(updateData);
        return HttpResponse.json({ success: true, data: { updated: 1 } });
      }),
    );
    const result = await wordStatesApi.batchUpdate(updateData as any);
    expect(result).toEqual({ updated: 1 });
  });

  it('markMastered marks a word as mastered', async () => {
    const mastered = { wordId: 'w1', level: 5, mastered: true };
    server.use(
      http.post(`${BASE}/api/word-states/w1/mark-mastered`, () =>
        HttpResponse.json({ success: true, data: mastered })),
    );
    const result = await wordStatesApi.markMastered('w1');
    expect(result).toEqual(mastered);
  });

  it('reset resets learning state for a word', async () => {
    const resetState = { wordId: 'w1', level: 0, nextReview: '2026-02-10' };
    server.use(
      http.post(`${BASE}/api/word-states/w1/reset`, () =>
        HttpResponse.json({ success: true, data: resetState })),
    );
    const result = await wordStatesApi.reset('w1');
    expect(result).toEqual(resetState);
  });
});
