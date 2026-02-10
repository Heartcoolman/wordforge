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

import { learningApi } from '@/api/learning';

const server = setupServer();
beforeAll(() => server.listen({ onUnhandledRequest: 'bypass' }));
afterEach(() => server.resetHandlers());
afterAll(() => server.close());

describe('learningApi', () => {
  it('createSession creates a new session', async () => {
    server.use(
      http.post(`${BASE}/api/learning/session`, () =>
        HttpResponse.json({ success: true, data: { sessionId: 'sess-1' } })),
    );
    const result = await learningApi.createSession();
    expect(result).toEqual({ sessionId: 'sess-1' });
  });

  it('getStudyWords fetches study words', async () => {
    const data = { words: [{ id: 'w1', text: 'test', meaning: '测试' }] };
    server.use(
      http.post(`${BASE}/api/learning/study-words`, () =>
        HttpResponse.json({ success: true, data })),
    );
    const result = await learningApi.getStudyWords();
    expect(result).toEqual(data);
  });

  it('getNextWords fetches next words', async () => {
    const data = { words: [{ id: 'w2', text: 'next', meaning: '下一个' }] };
    server.use(
      http.post(`${BASE}/api/learning/next-words`, () =>
        HttpResponse.json({ success: true, data })),
    );
    const result = await learningApi.getNextWords({ sessionId: 'sess-1', count: 5 });
    expect(result).toEqual(data);
  });

  it('adjustWords sends adjustment data', async () => {
    const adjusted = { adjustedStrategy: { difficulty: 3, pace: 'normal' } };
    server.use(
      http.post(`${BASE}/api/learning/adjust-words`, () =>
        HttpResponse.json({ success: true, data: adjusted })),
    );
    const result = await learningApi.adjustWords({ userState: 'focused', recentPerformance: 0.8 });
    expect(result).toEqual(adjusted);
  });

  it('syncProgress syncs learning progress', async () => {
    const session = { sessionId: 'sess-1', wordsCompleted: 10 };
    server.use(
      http.post(`${BASE}/api/learning/sync-progress`, () =>
        HttpResponse.json({ success: true, data: session })),
    );
    const result = await learningApi.syncProgress({ sessionId: 'sess-1', results: [] });
    expect(result).toEqual(session);
  });
});
