import { describe, it, expect, vi, beforeAll, afterAll, afterEach, beforeEach } from 'vitest';
import { setupServer } from 'msw/node';
import { http, HttpResponse } from 'msw';

const BASE = 'http://localhost:3000';

const server = setupServer();
beforeAll(() => server.listen({ onUnhandledRequest: 'bypass' }));
afterEach(() => server.resetHandlers());
afterAll(() => server.close());

vi.mock('@/lib/token', () => {
  let _token: string | null = null;
  let _adminToken: string | null = null;
  let _needsRefresh = false;
  return {
    tokenManager: {
      getToken: () => _token,
      getAdminToken: () => _adminToken,
      setTokens: vi.fn(),
      clearTokens: vi.fn(),
      needsRefresh: () => _needsRefresh,
      isAuthenticated: () => _token !== null,
      _set(t: string | null) { _token = t; },
      _setAdmin(t: string | null) { _adminToken = t; },
      _setNeedsRefresh(v: boolean) { _needsRefresh = v; },
    },
  };
});

vi.mock('@/api/auth', () => ({
  authApi: {
    refresh: vi.fn(),
  },
}));

import { ApiError, unauthorized, resetUnauthorized, api } from '@/api/client';
import { tokenManager } from '@/lib/token';
import { authApi } from '@/api/auth';

const tm = tokenManager as unknown as typeof tokenManager & {
  _set: (t: string | null) => void;
  _setAdmin: (t: string | null) => void;
  _setNeedsRefresh: (v: boolean) => void;
};

beforeEach(() => {
  tm._set(null);
  tm._setAdmin(null);
  tm._setNeedsRefresh(false);
  resetUnauthorized();
  vi.clearAllMocks();
});

describe('ApiError', () => {
  it('creates with status, code, message', () => {
    const err = new ApiError(404, 'NOT_FOUND', 'Not found');
    expect(err.status).toBe(404);
    expect(err.code).toBe('NOT_FOUND');
    expect(err.message).toBe('Not found');
    expect(err.name).toBe('ApiError');
  });

  it('includes optional traceId', () => {
    const err = new ApiError(500, 'ERR', 'fail', 'trace-123');
    expect(err.traceId).toBe('trace-123');
  });
});

describe('unauthorized signal', () => {
  it('defaults to false', () => {
    expect(unauthorized()).toBe(false);
  });

  it('resetUnauthorized sets back to false', () => {
    resetUnauthorized();
    expect(unauthorized()).toBe(false);
  });
});

describe('api.get', () => {
  it('sends GET request and unwraps success response', async () => {
    server.use(
      http.get(`${BASE}/api/data`, () =>
        HttpResponse.json({ success: true, data: { id: 1 } })),
    );
    const result = await api.get<{ id: number }>('/api/data', undefined, { skipTokenRefresh: true });
    expect(result).toEqual({ id: 1 });
  });

  it('passes query params correctly', async () => {
    server.use(
      http.get(`${BASE}/api/search`, ({ request }) => {
        const url = new URL(request.url);
        return HttpResponse.json({ success: true, data: url.searchParams.get('q') });
      }),
    );
    const result = await api.get<string>('/api/search', { q: 'hello' }, { skipTokenRefresh: true });
    expect(result).toBe('hello');
  });

  it('handles 204 No Content', async () => {
    server.use(
      http.get(`${BASE}/api/empty`, () => new HttpResponse(null, { status: 204 })),
    );
    const result = await api.get('/api/empty', undefined, { skipTokenRefresh: true });
    expect(result).toBeUndefined();
  });
});

describe('api.post', () => {
  it('sends POST with JSON body', async () => {
    server.use(
      http.post(`${BASE}/api/items`, async ({ request }) => {
        const body = await request.json() as Record<string, unknown>;
        return HttpResponse.json({ success: true, data: body });
      }),
    );
    const result = await api.post('/api/items', { name: 'test' }, { skipTokenRefresh: true });
    expect(result).toEqual({ name: 'test' });
  });

  it('sets Content-Type header', async () => {
    let contentType = '';
    server.use(
      http.post(`${BASE}/api/items`, ({ request }) => {
        contentType = request.headers.get('content-type') ?? '';
        return HttpResponse.json({ success: true, data: null });
      }),
    );
    await api.post('/api/items', { a: 1 }, { skipTokenRefresh: true });
    expect(contentType).toBe('application/json');
  });
});

describe('api.put', () => {
  it('sends PUT with JSON body', async () => {
    server.use(
      http.put(`${BASE}/api/items/1`, async ({ request }) => {
        const body = await request.json() as Record<string, unknown>;
        return HttpResponse.json({ success: true, data: body });
      }),
    );
    const result = await api.put('/api/items/1', { name: 'updated' }, { skipTokenRefresh: true });
    expect(result).toEqual({ name: 'updated' });
  });
});

describe('api.delete', () => {
  it('sends DELETE request', async () => {
    server.use(
      http.delete(`${BASE}/api/items/1`, () => new HttpResponse(null, { status: 204 })),
    );
    const result = await api.delete('/api/items/1', { skipTokenRefresh: true });
    expect(result).toBeUndefined();
  });
});

describe('Error handling', () => {
  it('throws ApiError on non-ok response', async () => {
    server.use(
      http.get(`${BASE}/api/fail`, () =>
        HttpResponse.json({ code: 'BAD', message: 'bad request' }, { status: 400 })),
    );
    await expect(api.get('/api/fail', undefined, { skipTokenRefresh: true }))
      .rejects.toBeInstanceOf(ApiError);
  });

  it('parses error body for code/message', async () => {
    server.use(
      http.get(`${BASE}/api/fail2`, () =>
        HttpResponse.json({ code: 'VALIDATION', message: 'Invalid input' }, { status: 422 })),
    );
    try {
      await api.get('/api/fail2', undefined, { skipTokenRefresh: true });
    } catch (e) {
      const err = e as ApiError;
      expect(err.code).toBe('VALIDATION');
      expect(err.message).toBe('Invalid input');
    }
  });

  it('sets unauthorized on 401', async () => {
    server.use(
      http.get(`${BASE}/api/auth-fail`, () =>
        HttpResponse.json({ code: 'UNAUTHORIZED', message: 'No auth' }, { status: 401 })),
    );
    try {
      await api.get('/api/auth-fail', undefined, { skipTokenRefresh: true });
    } catch { /* expected */ }
    expect(unauthorized()).toBe(true);
  });
});

describe('Token injection', () => {
  it('adds Authorization header when token exists', async () => {
    tm._set('my-token');
    let authHeader = '';
    server.use(
      http.get(`${BASE}/api/me`, ({ request }) => {
        authHeader = request.headers.get('authorization') ?? '';
        return HttpResponse.json({ success: true, data: null });
      }),
    );
    await api.get('/api/me', undefined, { skipTokenRefresh: true });
    expect(authHeader).toBe('Bearer my-token');
  });

  it('no Authorization header when no token', async () => {
    let authHeader: string | null = 'initial';
    server.use(
      http.get(`${BASE}/api/public`, ({ request }) => {
        authHeader = request.headers.get('authorization');
        return HttpResponse.json({ success: true, data: null });
      }),
    );
    await api.get('/api/public', undefined, { skipTokenRefresh: true });
    expect(authHeader).toBeNull();
  });

  it('uses admin token when useAdminToken is true', async () => {
    tm._setAdmin('admin-tok');
    let authHeader = '';
    server.use(
      http.get(`${BASE}/api/admin`, ({ request }) => {
        authHeader = request.headers.get('authorization') ?? '';
        return HttpResponse.json({ success: true, data: null });
      }),
    );
    await api.get('/api/admin', undefined, { useAdminToken: true });
    expect(authHeader).toBe('Bearer admin-tok');
  });
});

describe('Token refresh', () => {
  it('calls refresh when token needs refresh', async () => {
    tm._set('old-token');
    tm._setNeedsRefresh(true);
    (authApi.refresh as ReturnType<typeof vi.fn>).mockResolvedValueOnce({
      accessToken: 'new-access',
      refreshToken: 'new-refresh',
    });
    server.use(
      http.get(`${BASE}/api/data`, () =>
        HttpResponse.json({ success: true, data: 'ok' })),
    );
    await api.get('/api/data');
    expect(authApi.refresh).toHaveBeenCalledOnce();
  });

  it('multiple concurrent requests share single refresh', async () => {
    let refreshCount = 0;
    tm._set('old-token');
    tm._setNeedsRefresh(true);
    (authApi.refresh as ReturnType<typeof vi.fn>).mockImplementation(async () => {
      refreshCount++;
      return { accessToken: 'new', refreshToken: 'new-r' };
    });
    server.use(
      http.get(`${BASE}/api/a`, () => HttpResponse.json({ success: true, data: 1 })),
      http.get(`${BASE}/api/b`, () => HttpResponse.json({ success: true, data: 2 })),
    );
    await Promise.all([api.get('/api/a'), api.get('/api/b')]);
    expect(refreshCount).toBe(1);
  });

  it('sets unauthorized when refresh fails', async () => {
    tm._set('old-token');
    tm._setNeedsRefresh(true);
    (authApi.refresh as ReturnType<typeof vi.fn>).mockRejectedValueOnce(new Error('fail'));
    server.use(
      http.get(`${BASE}/api/data`, () =>
        HttpResponse.json({ success: true, data: 'ok' })),
    );
    await api.get('/api/data');
    expect(tokenManager.clearTokens).toHaveBeenCalled();
  });
});

describe('Timeout', () => {
  it('aborts request on timeout', async () => {
    server.use(
      http.get(`${BASE}/api/slow`, async () => {
        await new Promise((r) => setTimeout(r, 5000));
        return HttpResponse.json({ success: true, data: null });
      }),
    );
    await expect(
      api.get('/api/slow', undefined, { timeout: 50, skipTokenRefresh: true }),
    ).rejects.toThrow();
  });
});

describe('unwrap logic', () => {
  it('unwraps {success:true, data} envelope', async () => {
    server.use(
      http.get(`${BASE}/api/ok`, () =>
        HttpResponse.json({ success: true, data: { value: 42 } })),
    );
    const result = await api.get<{ value: number }>('/api/ok', undefined, { skipTokenRefresh: true });
    expect(result).toEqual({ value: 42 });
  });

  it('throws on {success:false} envelope', async () => {
    server.use(
      http.get(`${BASE}/api/fail`, () =>
        HttpResponse.json({ success: false, code: 'FAIL', message: 'nope' })),
    );
    await expect(api.get('/api/fail', undefined, { skipTokenRefresh: true }))
      .rejects.toThrow('nope');
  });

  it('returns raw JSON when no success field', async () => {
    server.use(
      http.get(`${BASE}/api/raw`, () =>
        HttpResponse.json({ items: [1, 2, 3] })),
    );
    const result = await api.get<{ items: number[] }>('/api/raw', undefined, { skipTokenRefresh: true });
    expect(result).toEqual({ items: [1, 2, 3] });
  });
});
