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

import { contentApi } from '@/api/content';

describe('contentApi', () => {
  it('getEtymology returns etymology for a word', async () => {
    const etymology = { wordId: 'w1', origin: 'Latin', history: 'From "testare"' };
    server.use(
      http.get(`${BASE}/api/content/etymology/w1`, () =>
        HttpResponse.json({ success: true, data: etymology })),
    );
    const result = await contentApi.getEtymology('w1');
    expect(result).toEqual(etymology);
  });

  it('semanticSearch sends query and limit as params', async () => {
    const searchResult = { results: [{ wordId: 'w1', score: 0.95 }] };
    server.use(
      http.get(`${BASE}/api/content/semantic/search`, ({ request }) => {
        const url = new URL(request.url);
        expect(url.searchParams.get('query')).toBe('happy');
        expect(url.searchParams.get('limit')).toBe('5');
        return HttpResponse.json({ success: true, data: searchResult });
      }),
    );
    const result = await contentApi.semanticSearch('happy', 5);
    expect(result).toEqual(searchResult);
  });

  it('semanticSearch uses default limit of 10', async () => {
    const searchResult = { results: [] };
    server.use(
      http.get(`${BASE}/api/content/semantic/search`, ({ request }) => {
        const url = new URL(request.url);
        expect(url.searchParams.get('limit')).toBe('10');
        return HttpResponse.json({ success: true, data: searchResult });
      }),
    );
    const result = await contentApi.semanticSearch('test');
    expect(result).toEqual(searchResult);
  });

  it('getWordContexts returns contexts for a word', async () => {
    const contexts = { wordId: 'w1', sentences: ['The test was easy.'] };
    server.use(
      http.get(`${BASE}/api/content/word-contexts/w1`, () =>
        HttpResponse.json({ success: true, data: contexts })),
    );
    const result = await contentApi.getWordContexts('w1');
    expect(result).toEqual(contexts);
  });

  it('getMorphemes returns morphemes for a word', async () => {
    const morphemes = { wordId: 'w1', morphemes: [{ type: 'root', value: 'test' }] };
    server.use(
      http.get(`${BASE}/api/content/morphemes/w1`, () =>
        HttpResponse.json({ success: true, data: morphemes })),
    );
    const result = await contentApi.getMorphemes('w1');
    expect(result).toEqual(morphemes);
  });

  it('getConfusionPairs returns confusion pairs for a word', async () => {
    const pairs = { wordId: 'w1', pairs: [{ word: 'affect', confusedWith: 'effect', score: 0.8 }] };
    server.use(
      http.get(`${BASE}/api/content/confusion-pairs/w1`, () =>
        HttpResponse.json({ success: true, data: pairs })),
    );
    const result = await contentApi.getConfusionPairs('w1');
    expect(result).toEqual(pairs);
  });
});
