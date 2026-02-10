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

import { wordbooksApi } from '@/api/wordbooks';

describe('wordbooksApi', () => {
  it('getSystem returns system wordbooks', async () => {
    const books = [{ id: 'b1', name: 'CET-4', type: 'system', wordCount: 4000 }];
    server.use(
      http.get(`${BASE}/api/wordbooks/system`, () =>
        HttpResponse.json({ success: true, data: books })),
    );
    const result = await wordbooksApi.getSystem();
    expect(result).toEqual(books);
  });

  it('getUser returns user wordbooks', async () => {
    const books = [{ id: 'b2', name: 'My Words', type: 'user', wordCount: 50 }];
    server.use(
      http.get(`${BASE}/api/wordbooks/user`, () =>
        HttpResponse.json({ success: true, data: books })),
    );
    const result = await wordbooksApi.getUser();
    expect(result).toEqual(books);
  });

  it('create sends wordbook data and returns created wordbook', async () => {
    const newBook = { id: 'b3', name: 'Test Book', type: 'user', wordCount: 0 };
    server.use(
      http.post(`${BASE}/api/wordbooks`, async ({ request }) => {
        const body = await request.json() as Record<string, unknown>;
        expect(body).toEqual({ name: 'Test Book' });
        return HttpResponse.json({ success: true, data: newBook });
      }),
    );
    const result = await wordbooksApi.create({ name: 'Test Book' } as any);
    expect(result).toEqual(newBook);
  });

  it('getWords returns paginated words for a wordbook', async () => {
    const response = { items: [{ id: 'w1', word: 'test' }], total: 1, limit: 20, offset: 0 };
    server.use(
      http.get(`${BASE}/api/wordbooks/b1/words`, () =>
        HttpResponse.json({ success: true, data: response })),
    );
    const result = await wordbooksApi.getWords('b1');
    expect(result).toEqual(response);
  });

  it('getWords sends pagination params', async () => {
    const response = { items: [], total: 100, limit: 10, offset: 20 };
    server.use(
      http.get(`${BASE}/api/wordbooks/b1/words`, ({ request }) => {
        const url = new URL(request.url);
        expect(url.searchParams.get('limit')).toBe('10');
        expect(url.searchParams.get('offset')).toBe('20');
        return HttpResponse.json({ success: true, data: response });
      }),
    );
    const result = await wordbooksApi.getWords('b1', { limit: 10, offset: 20 });
    expect(result).toEqual(response);
  });

  it('addWords sends wordIds and returns added count', async () => {
    server.use(
      http.post(`${BASE}/api/wordbooks/b1/words`, async ({ request }) => {
        const body = await request.json() as Record<string, unknown>;
        expect(body).toEqual({ wordIds: ['w1', 'w2'] });
        return HttpResponse.json({ success: true, data: { added: 2 } });
      }),
    );
    const result = await wordbooksApi.addWords('b1', ['w1', 'w2']);
    expect(result).toEqual({ added: 2 });
  });

  it('removeWord deletes a word from a wordbook', async () => {
    server.use(
      http.delete(`${BASE}/api/wordbooks/b1/words/w1`, () =>
        HttpResponse.json({ success: true, data: { removed: true } })),
    );
    const result = await wordbooksApi.removeWord('b1', 'w1');
    expect(result).toEqual({ removed: true });
  });
});
