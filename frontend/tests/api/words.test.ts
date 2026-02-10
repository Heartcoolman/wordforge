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

import { wordsApi } from '@/api/words';

describe('wordsApi', () => {
  it('list returns paginated words', async () => {
    const response = { items: [{ id: 'w1', word: 'test', definition: 'a trial' }], total: 1, limit: 20, offset: 0 };
    server.use(
      http.get(`${BASE}/api/words`, () =>
        HttpResponse.json({ success: true, data: response })),
    );
    const result = await wordsApi.list();
    expect(result).toEqual(response);
  });

  it('list sends query params', async () => {
    const response = { items: [], total: 0, limit: 10, offset: 0 };
    server.use(
      http.get(`${BASE}/api/words`, ({ request }) => {
        const url = new URL(request.url);
        expect(url.searchParams.get('limit')).toBe('10');
        expect(url.searchParams.get('offset')).toBe('5');
        expect(url.searchParams.get('search')).toBe('hello');
        return HttpResponse.json({ success: true, data: response });
      }),
    );
    const result = await wordsApi.list({ limit: 10, offset: 5, search: 'hello' });
    expect(result).toEqual(response);
  });

  it('get returns a single word by id', async () => {
    const word = { id: 'w1', word: 'test', definition: 'a trial' };
    server.use(
      http.get(`${BASE}/api/words/w1`, () =>
        HttpResponse.json({ success: true, data: word })),
    );
    const result = await wordsApi.get('w1');
    expect(result).toEqual(word);
  });

  it('create sends word data and returns created word', async () => {
    const newWord = { id: 'w2', word: 'hello', definition: 'a greeting' };
    server.use(
      http.post(`${BASE}/api/words`, async ({ request }) => {
        const body = await request.json() as Record<string, unknown>;
        expect(body).toEqual({ word: 'hello', definition: 'a greeting' });
        return HttpResponse.json({ success: true, data: newWord });
      }),
    );
    const result = await wordsApi.create({ word: 'hello', definition: 'a greeting' } as any);
    expect(result).toEqual(newWord);
  });

  it('update sends updated data and returns updated word', async () => {
    const updated = { id: 'w1', word: 'test', definition: 'updated definition' };
    server.use(
      http.put(`${BASE}/api/words/w1`, async ({ request }) => {
        const body = await request.json() as Record<string, unknown>;
        expect(body).toEqual({ word: 'test', definition: 'updated definition' });
        return HttpResponse.json({ success: true, data: updated });
      }),
    );
    const result = await wordsApi.update('w1', { word: 'test', definition: 'updated definition' } as any);
    expect(result).toEqual(updated);
  });

  it('delete removes a word and returns confirmation', async () => {
    server.use(
      http.delete(`${BASE}/api/words/w1`, () =>
        HttpResponse.json({ success: true, data: { deleted: true, id: 'w1' } })),
    );
    const result = await wordsApi.delete('w1');
    expect(result).toEqual({ deleted: true, id: 'w1' });
  });

  it('batchCreate sends array of words and returns batch result', async () => {
    const batchResult = { created: 2, errors: [] };
    const words = [
      { word: 'foo', definition: 'bar' },
      { word: 'baz', definition: 'qux' },
    ];
    server.use(
      http.post(`${BASE}/api/words/batch`, async ({ request }) => {
        const body = await request.json() as Record<string, unknown>;
        expect(body).toEqual({ words });
        return HttpResponse.json({ success: true, data: batchResult });
      }),
    );
    const result = await wordsApi.batchCreate(words as any);
    expect(result).toEqual(batchResult);
  });

  it('count returns total word count', async () => {
    server.use(
      http.get(`${BASE}/api/words/count`, () =>
        HttpResponse.json({ success: true, data: { total: 5000 } })),
    );
    const result = await wordsApi.count();
    expect(result).toEqual({ total: 5000 });
  });

  it('importUrl sends URL and returns import result', async () => {
    const importResult = { imported: 25, skipped: 3, errors: [] };
    server.use(
      http.post(`${BASE}/api/words/import-url`, async ({ request }) => {
        const body = await request.json() as Record<string, unknown>;
        expect(body).toEqual({ url: 'https://example.com/words.json' });
        return HttpResponse.json({ success: true, data: importResult });
      }),
    );
    const result = await wordsApi.importUrl('https://example.com/words.json');
    expect(result).toEqual(importResult);
  });
});
