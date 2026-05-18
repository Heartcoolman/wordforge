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

import { wordbookCenterApi } from '@/api/wordbookCenter';

const server = setupServer();
beforeAll(() => server.listen({ onUnhandledRequest: 'bypass' }));
afterEach(() => server.resetHandlers());
afterAll(() => server.close());

describe('wordbookCenterApi', () => {
  it('getSettings returns user wordbook center settings', async () => {
    const settings = { wordbookCenterUrl: 'https://wbc.example' };
    server.use(
      http.get(`${BASE}/api/wordbook-center/settings`, () =>
        HttpResponse.json({ success: true, data: settings })),
    );
    const result = await wordbookCenterApi.getSettings();
    expect(result).toEqual(settings);
  });

  it('updateSettings sends PUT with payload', async () => {
    const updated = { wordbookCenterUrl: 'https://new.example' };
    server.use(
      http.put(`${BASE}/api/wordbook-center/settings`, async ({ request }) => {
        const body = await request.json() as Record<string, unknown>;
        expect(body).toEqual({ wordbookCenterUrl: 'https://new.example' });
        return HttpResponse.json({ success: true, data: updated });
      }),
    );
    const result = await wordbookCenterApi.updateSettings({ wordbookCenterUrl: 'https://new.example' });
    expect(result).toEqual(updated);
  });

  it('browse returns wordbook list', async () => {
    const items = [{ id: 'wb1', title: 'Book One', wordCount: 100, version: '1.0', publishedAt: '2026-01-01' }];
    server.use(
      http.get(`${BASE}/api/wordbook-center/browse`, () =>
        HttpResponse.json({ success: true, data: items })),
    );
    const result = await wordbookCenterApi.browse();
    expect(result).toEqual(items);
  });

  it('preview forwards pagination query params', async () => {
    const preview = { id: 'wb1', title: 'Book', words: [], total: 0 };
    server.use(
      http.get(`${BASE}/api/wordbook-center/browse/wb1`, ({ request }) => {
        const url = new URL(request.url);
        expect(url.searchParams.get('page')).toBe('2');
        expect(url.searchParams.get('perPage')).toBe('25');
        return HttpResponse.json({ success: true, data: preview });
      }),
    );
    const result = await wordbookCenterApi.preview('wb1', { page: 2, perPage: 25 });
    expect(result).toEqual(preview);
  });

  it('preview omits params when none provided', async () => {
    const preview = { id: 'wb1', title: 'Book', words: [], total: 0 };
    server.use(
      http.get(`${BASE}/api/wordbook-center/browse/wb1`, ({ request }) => {
        const url = new URL(request.url);
        expect(url.searchParams.has('page')).toBe(false);
        return HttpResponse.json({ success: true, data: preview });
      }),
    );
    await wordbookCenterApi.preview('wb1');
  });

  it('import POSTs by id and returns import result', async () => {
    const importResult = { imported: 10, skipped: 0, errors: [] };
    server.use(
      http.post(`${BASE}/api/wordbook-center/import/wb-7`, () =>
        HttpResponse.json({ success: true, data: importResult })),
    );
    const result = await wordbookCenterApi.import('wb-7');
    expect(result).toEqual(importResult);
  });

  it('importUrl POSTs the URL body', async () => {
    const importResult = { imported: 5, skipped: 1, errors: [] };
    server.use(
      http.post(`${BASE}/api/wordbook-center/import-url`, async ({ request }) => {
        const body = await request.json() as Record<string, unknown>;
        expect(body).toEqual({ url: 'https://wbc.example/x.json' });
        return HttpResponse.json({ success: true, data: importResult });
      }),
    );
    const result = await wordbookCenterApi.importUrl('https://wbc.example/x.json');
    expect(result).toEqual(importResult);
  });

  it('getUpdates returns update info list', async () => {
    const updates = [{ id: 'wb1', oldVersion: '1.0', newVersion: '2.0', changes: 12 }];
    server.use(
      http.get(`${BASE}/api/wordbook-center/updates`, () =>
        HttpResponse.json({ success: true, data: updates })),
    );
    const result = await wordbookCenterApi.getUpdates();
    expect(result).toEqual(updates);
  });

  it('sync POSTs to the update endpoint for the wordbook id', async () => {
    const syncResult = { updated: 8, added: 2, removed: 0 };
    server.use(
      http.post(`${BASE}/api/wordbook-center/updates/wb-9/sync`, () =>
        HttpResponse.json({ success: true, data: syncResult })),
    );
    const result = await wordbookCenterApi.sync('wb-9');
    expect(result).toEqual(syncResult);
  });
});
