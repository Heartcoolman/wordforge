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

import { studyConfigApi } from '@/api/studyConfig';

const server = setupServer();
beforeAll(() => server.listen({ onUnhandledRequest: 'bypass' }));
afterEach(() => server.resetHandlers());
afterAll(() => server.close());

describe('studyConfigApi', () => {
  it('get fetches study config', async () => {
    const config = { dailyGoal: 20, selectedWordbookIds: ['book1'] };
    server.use(
      http.get(`${BASE}/api/study-config`, () =>
        HttpResponse.json({ success: true, data: config })),
    );
    const result = await studyConfigApi.get();
    expect(result).toEqual(config);
  });

  it('update sends PUT with config data', async () => {
    const updated = { dailyGoal: 30, selectedWordbookIds: ['book1', 'book2'] };
    server.use(
      http.put(`${BASE}/api/study-config`, () =>
        HttpResponse.json({ success: true, data: updated })),
    );
    const result = await studyConfigApi.update({ selectedWordbookIds: ['book1', 'book2'] });
    expect(result).toEqual(updated);
  });

  it('getTodayWords fetches today words', async () => {
    const data = { words: [{ id: 'w1', text: 'hello', meaning: '你好' }], target: 20 };
    server.use(
      http.get(`${BASE}/api/study-config/today-words`, () =>
        HttpResponse.json({ success: true, data })),
    );
    const result = await studyConfigApi.getTodayWords();
    expect(result).toEqual(data);
  });

  it('getProgress fetches study progress', async () => {
    const progress = { completed: 15, total: 20, percentage: 0.75 };
    server.use(
      http.get(`${BASE}/api/study-config/progress`, () =>
        HttpResponse.json({ success: true, data: progress })),
    );
    const result = await studyConfigApi.getProgress();
    expect(result).toEqual(progress);
  });
});
