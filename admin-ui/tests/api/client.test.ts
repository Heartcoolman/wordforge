import { describe, it, expect, vi, beforeAll, afterAll, afterEach, beforeEach } from 'vitest';
import { setupServer } from 'msw/node';
import { http, HttpResponse } from 'msw';

import { TEST_BASE_URL as BASE } from '../helpers/constants';

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
      clearAdminToken: vi.fn(),
      needsRefresh: () => _needsRefresh,
      isAuthenticated: () => _token !== null,
      refreshAccessToken: vi.fn(),
      _set(t: string | null) { _token = t; },
      _setAdmin(t: string | null) { _adminToken = t; },
      _setNeedsRefresh(v: boolean) { _needsRefresh = v; },
    },
  };
});

import {
  ApiError,
  unauthorized,
  resetUnauthorized,
  api,
  maintenanceActive,
  setMaintenanceActive,
  updateInfo,
  setUpdateInfo,
  connectSseStream,
  connectAmasStateStream,
} from '@/api/http';
import { tokenManager } from '@/lib/token';

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
  vi.unstubAllGlobals();
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

  it('accepts a path without leading slash and normalizes it', async () => {
    server.use(
      http.get(`${BASE}/api/no-slash`, () =>
        HttpResponse.json({ success: true, data: 'ok' })),
    );
    const result = await api.get<string>('api/no-slash', undefined, { skipTokenRefresh: true });
    expect(result).toBe('ok');
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

  it('clears admin token and emits event on admin 401', async () => {
    tm._setAdmin('admin-token');
    const onUnauthorized = vi.fn();
    window.addEventListener('admin:unauthorized', onUnauthorized);

    server.use(
      http.get(`${BASE}/api/admin/fail`, () =>
        HttpResponse.json({ code: 'AUTH_UNAUTHORIZED', message: 'expired' }, { status: 401 })),
    );

    try {
      await api.get('/api/admin/fail', undefined, { useAdminToken: true });
    } catch { /* expected */ }

    expect(tokenManager.clearAdminToken).toHaveBeenCalledTimes(1);
    expect(tokenManager.clearTokens).not.toHaveBeenCalled();
    expect(onUnauthorized).toHaveBeenCalledTimes(1);
    expect(unauthorized()).toBe(false);

    window.removeEventListener('admin:unauthorized', onUnauthorized);
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
  it('calls refreshAccessToken when token needs refresh', async () => {
    tm._set('old-token');
    tm._setNeedsRefresh(true);
    (tokenManager.refreshAccessToken as ReturnType<typeof vi.fn>).mockResolvedValueOnce(true);
    server.use(
      http.get(`${BASE}/api/data`, () =>
        HttpResponse.json({ success: true, data: 'ok' })),
    );
    await api.get('/api/data');
    expect(tokenManager.refreshAccessToken).toHaveBeenCalledOnce();
  });

  it('multiple concurrent requests each call refreshAccessToken', async () => {
    tm._set('old-token');
    tm._setNeedsRefresh(true);
    (tokenManager.refreshAccessToken as ReturnType<typeof vi.fn>).mockResolvedValue(true);
    server.use(
      http.get(`${BASE}/api/a`, () => HttpResponse.json({ success: true, data: 1 })),
      http.get(`${BASE}/api/b`, () => HttpResponse.json({ success: true, data: 2 })),
    );
    await Promise.all([api.get('/api/a'), api.get('/api/b')]);
    expect(tokenManager.refreshAccessToken).toHaveBeenCalled();
  });

  it('clears tokens when refresh fails', async () => {
    tm._set('old-token');
    tm._setNeedsRefresh(true);
    (tokenManager.refreshAccessToken as ReturnType<typeof vi.fn>).mockResolvedValueOnce(false);
    server.use(
      http.get(`${BASE}/api/data`, () =>
        HttpResponse.json({ success: true, data: 'ok' })),
    );
    await api.get('/api/data');
    expect(tokenManager.refreshAccessToken).toHaveBeenCalled();
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

describe('unwrap edge cases', () => {
  it('falls back to "API_ERROR" code and uses error field when message missing in success-envelope', async () => {
    server.use(
      http.get(`${BASE}/api/envelope-fail`, () =>
        HttpResponse.json({ success: false, error: 'fallback-error' })),
    );
    try {
      await api.get('/api/envelope-fail', undefined, { skipTokenRefresh: true });
    } catch (e) {
      const err = e as ApiError;
      expect(err.code).toBe('API_ERROR');
      expect(err.message).toBe('fallback-error');
    }
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

  it('parses error JSON without code/message and falls back to statusText', async () => {
    server.use(
      http.get(`${BASE}/api/empty-err`, () => new HttpResponse('not-json', { status: 500 })),
    );
    try {
      await api.get('/api/empty-err', undefined, { skipTokenRefresh: true });
      throw new Error('should have thrown');
    } catch (e) {
      const err = e as ApiError;
      expect(err.status).toBe(500);
      expect(err.code).toBe('UNKNOWN');
    }
  });

  it('uses error field as message when message missing', async () => {
    server.use(
      http.get(`${BASE}/api/err-field`, () =>
        HttpResponse.json({ code: 'ERR', error: 'fallback error msg' }, { status: 400 })),
    );
    try {
      await api.get('/api/err-field', undefined, { skipTokenRefresh: true });
    } catch (e) {
      expect((e as ApiError).message).toBe('fallback error msg');
    }
  });

  it('returns undefined when content-length is 0', async () => {
    server.use(
      http.get(`${BASE}/api/zero`, () => new HttpResponse(null, { status: 200, headers: { 'content-length': '0' } })),
    );
    const result = await api.get('/api/zero', undefined, { skipTokenRefresh: true });
    expect(result).toBeUndefined();
  });
});

describe('429 rate limit handling', () => {
  it('parses Retry-After header into ApiError', async () => {
    server.use(
      http.get(`${BASE}/api/throttled`, () =>
        HttpResponse.json({ code: 'RATE_LIMITED', message: 'slow down' }, { status: 429, headers: { 'Retry-After': '12' } })),
    );
    try {
      await api.get('/api/throttled', undefined, { skipTokenRefresh: true });
    } catch (e) {
      const err = e as ApiError;
      expect(err.status).toBe(429);
      expect(err.retryAfter).toBe(12);
      expect(err.message).toContain('12');
    }
  });

  it('falls back to generic message when Retry-After absent', async () => {
    server.use(
      http.get(`${BASE}/api/throttled2`, () =>
        HttpResponse.json({}, { status: 429 })),
    );
    try {
      await api.get('/api/throttled2', undefined, { skipTokenRefresh: true });
    } catch (e) {
      const err = e as ApiError;
      expect(err.retryAfter).toBeUndefined();
      expect(err.message).toBe('请求过于频繁，请稍后重试');
    }
  });

  it('falls back to statusText when error body has no code/message/error', async () => {
    server.use(
      http.get(`${BASE}/api/blank-err`, () =>
        HttpResponse.json({}, { status: 503 })),
    );
    try {
      await api.get('/api/blank-err', undefined, { skipTokenRefresh: true });
    } catch (e) {
      const err = e as ApiError;
      expect(err.code).toBe('UNKNOWN');
      // statusText 是 'Service Unavailable' (HTTP 503)
      expect(err.message.length).toBeGreaterThan(0);
    }
  });
});

describe('401 unauthorized retry', () => {
  it('does not retry public /api/auth/* paths', async () => {
    tm._set('access');
    let attempts = 0;
    server.use(
      http.post(`${BASE}/api/auth/login`, () => {
        attempts += 1;
        return HttpResponse.json({ code: 'BAD', message: 'bad' }, { status: 401 });
      }),
    );
    await expect(api.post('/api/auth/login', { a: 1 })).rejects.toBeInstanceOf(ApiError);
    expect(attempts).toBe(1);
    expect(tokenManager.refreshAccessToken).not.toHaveBeenCalled();
  });

  it('retries with refreshed token on 401 success path', async () => {
    tm._set('access');
    (tokenManager.refreshAccessToken as ReturnType<typeof vi.fn>).mockResolvedValueOnce(true);
    let calls = 0;
    server.use(
      http.get(`${BASE}/api/needs-refresh`, () => {
        calls += 1;
        if (calls === 1) {
          return HttpResponse.json({ code: 'AUTH', message: 'expired' }, { status: 401 });
        }
        return HttpResponse.json({ success: true, data: 'after-refresh' });
      }),
    );
    const result = await api.get('/api/needs-refresh');
    expect(result).toBe('after-refresh');
    expect(calls).toBe(2);
  });

  it('sets unauthorized signal when refresh fails on 401', async () => {
    tm._set('access');
    (tokenManager.refreshAccessToken as ReturnType<typeof vi.fn>).mockResolvedValueOnce(false);
    server.use(
      http.get(`${BASE}/api/needs-refresh2`, () =>
        HttpResponse.json({ code: 'AUTH', message: 'expired' }, { status: 401 })),
    );
    await expect(api.get('/api/needs-refresh2')).rejects.toBeInstanceOf(ApiError);
    expect(unauthorized()).toBe(true);
  });
});

describe('error normalization', () => {
  it('wraps TypeError network failure into NETWORK_ERROR ApiError', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockRejectedValueOnce(new TypeError('Failed to fetch'));
    try {
      await api.get('/api/x', undefined, { skipTokenRefresh: true });
      throw new Error('should throw');
    } catch (e) {
      const err = e as ApiError;
      expect(err.code).toBe('NETWORK_ERROR');
      expect(err.status).toBe(0);
    }
    fetchSpy.mockRestore();
  });

  it('rethrows generic errors that are not ApiError/AbortError/TypeError', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockRejectedValueOnce(new RangeError('weird'));
    await expect(api.get('/api/y', undefined, { skipTokenRefresh: true })).rejects.toBeInstanceOf(RangeError);
    fetchSpy.mockRestore();
  });

  it('wraps AbortError into TIMEOUT ApiError', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockRejectedValueOnce(
      new DOMException('aborted', 'AbortError'),
    );
    try {
      await api.get('/api/abort-direct', undefined, { skipTokenRefresh: true });
    } catch (e) {
      expect((e as ApiError).code).toBe('TIMEOUT');
    }
    fetchSpy.mockRestore();
  });
});

describe('post/put with pre-set body (FormData path)', () => {
  it('post passes through body when opts.body present', async () => {
    let receivedType = '';
    server.use(
      http.post(`${BASE}/api/raw-post`, ({ request }) => {
        receivedType = request.headers.get('content-type') ?? '';
        return HttpResponse.json({ success: true, data: 'ok' });
      }),
    );
    const buf = new ArrayBuffer(4);
    const result = await api.post('/api/raw-post', undefined, {
      body: buf,
      headers: { 'Content-Type': 'application/octet-stream' },
      skipTokenRefresh: true,
    });
    expect(result).toBe('ok');
    expect(receivedType).toBe('application/octet-stream');
  });

  it('put passes through body when opts.body present', async () => {
    let bodyText = '';
    server.use(
      http.put(`${BASE}/api/raw-put`, async ({ request }) => {
        bodyText = await request.text();
        return HttpResponse.json({ success: true, data: 'ok' });
      }),
    );
    await api.put('/api/raw-put', undefined, {
      body: 'plain-text',
      skipTokenRefresh: true,
    });
    expect(bodyText).toBe('plain-text');
  });

  it('post without body sends no body field', async () => {
    let bodyText = 'sentinel';
    server.use(
      http.post(`${BASE}/api/no-body`, async ({ request }) => {
        bodyText = await request.text();
        return HttpResponse.json({ success: true, data: null });
      }),
    );
    await api.post('/api/no-body', undefined, { skipTokenRefresh: true });
    expect(bodyText).toBe('');
  });

  it('put without body sends no body field', async () => {
    let bodyText = 'sentinel';
    server.use(
      http.put(`${BASE}/api/no-body-put`, async ({ request }) => {
        bodyText = await request.text();
        return HttpResponse.json({ success: true, data: null });
      }),
    );
    await api.put('/api/no-body-put', undefined, { skipTokenRefresh: true });
    expect(bodyText).toBe('');
  });
});

describe('reactive signals', () => {
  it('maintenanceActive defaults to false and is settable', () => {
    expect(maintenanceActive()).toBe(false);
    setMaintenanceActive(true);
    expect(maintenanceActive()).toBe(true);
    setMaintenanceActive(false);
  });

  it('updateInfo defaults to null and is settable', () => {
    expect(updateInfo()).toBeNull();
    setUpdateInfo({ version: '1.0', message: 'note' });
    expect(updateInfo()).toEqual({ version: '1.0', message: 'note' });
    setUpdateInfo(null);
  });
});

describe('connectSseStream', () => {
  // 通过劫持 fetch 提供受控 ReadableStream，逐行投递 SSE 事件。
  function makeSseResponse(events: string[]): Response {
    const encoder = new TextEncoder();
    const stream = new ReadableStream({
      start(controller) {
        for (const e of events) controller.enqueue(encoder.encode(e));
        controller.close();
      },
    });
    return new Response(stream, { status: 200, headers: { 'Content-Type': 'text/event-stream' } });
  }

  it('parses amas_state and maintenance/telemetry/update events and invokes callbacks', async () => {
    tm._set('tok');
    const events = [
      'event: amas_state\ndata: {"attention":0.5,"fatigue":0.1,"motivation":0.2,"confidence":0.3,"sessionEventCount":1,"totalEventCount":2}\n\n',
      'event: maintenance\ndata: {"active":true}\n\n',
      'event: telemetry_request\ndata: {"requestId":"req-9"}\n\n',
      'event: update_available\ndata: {"version":"1.2.0","message":"new"}\n\n',
      'event: release_available\ndata: {"latestTag":"v1.3","channel":"beta"}\n\n',
      'event: update_progress\ndata: {"phase":"download","percent":"42"}\n\n',
      'event: data_corrupted\ndata: {}\n\n',
    ];
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce(makeSseResponse(events));

    const onAmasState = vi.fn();
    const onMaintenance = vi.fn();
    const onTelemetryRequest = vi.fn();
    const onUpdateAvailable = vi.fn();
    const onReleaseAvailable = vi.fn();
    const onUpdateProgress = vi.fn();
    const onDataCorrupted = vi.fn();

    const dispose = connectSseStream({
      onAmasState,
      onMaintenance,
      onTelemetryRequest,
      onUpdateAvailable,
      onReleaseAvailable,
      onUpdateProgress,
      onDataCorrupted,
    });

    // 等待事件循环把所有事件分发完
    await new Promise((r) => setTimeout(r, 30));
    dispose();

    expect(onAmasState).toHaveBeenCalledTimes(1);
    expect(onMaintenance).toHaveBeenCalledWith(true);
    expect(maintenanceActive()).toBe(true);
    expect(onTelemetryRequest).toHaveBeenCalledWith('req-9');
    expect(onUpdateAvailable).toHaveBeenCalledWith({ version: '1.2.0', message: 'new' });
    expect(updateInfo()).toEqual({ version: '1.2.0', message: 'new' });
    expect(onReleaseAvailable).toHaveBeenCalledWith({ latestTag: 'v1.3', channel: 'beta' });
    expect(onUpdateProgress).toHaveBeenCalledWith({ phase: 'download', percent: 42 });
    expect(onDataCorrupted).toHaveBeenCalledTimes(1);

    fetchSpy.mockRestore();
    setMaintenanceActive(false);
    setUpdateInfo(null);
  });

  it('ignores malformed JSON event data', async () => {
    const events = [
      'event: amas_state\ndata: {not-json\n\n',
    ];
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce(makeSseResponse(events));
    const onAmasState = vi.fn();
    const dispose = connectSseStream({ onAmasState });
    await new Promise((r) => setTimeout(r, 30));
    dispose();
    expect(onAmasState).not.toHaveBeenCalled();
    fetchSpy.mockRestore();
  });

  it('rejects invalid amas_state payload missing required numeric fields', async () => {
    const events = [
      'event: amas_state\ndata: {"attention":"oops"}\n\n',
    ];
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce(makeSseResponse(events));
    const onAmasState = vi.fn();
    const dispose = connectSseStream({ onAmasState });
    await new Promise((r) => setTimeout(r, 30));
    dispose();
    expect(onAmasState).not.toHaveBeenCalled();
    fetchSpy.mockRestore();
  });

  it('triggers token refresh before connect when needsRefresh is true', async () => {
    tm._setNeedsRefresh(true);
    (tokenManager.refreshAccessToken as ReturnType<typeof vi.fn>).mockResolvedValueOnce(true);
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce(makeSseResponse([]));
    const dispose = connectSseStream({});
    await new Promise((r) => setTimeout(r, 20));
    dispose();
    expect(tokenManager.refreshAccessToken).toHaveBeenCalled();
    fetchSpy.mockRestore();
  });

  it('connectAmasStateStream wires onState callback through to SSE handler', () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockResolvedValue(makeSseResponse([]));
    const cb = vi.fn();
    const dispose = connectAmasStateStream(cb);
    dispose();
    expect(typeof dispose).toBe('function');
    fetchSpy.mockRestore();
  });

  it('non-ok SSE response triggers reconnect loop (dispose stops it)', async () => {
    tm._set(null);
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response('err', { status: 500 }),
    );
    const dispose = connectSseStream({});
    await new Promise((r) => setTimeout(r, 30));
    dispose();
    expect(fetchSpy).toHaveBeenCalled();
    fetchSpy.mockRestore();
  });

  it('handles events when callbacks are not provided (optional chaining branch)', async () => {
    const events = [
      'event: maintenance\ndata: {"active":false}\n\n',
      'event: telemetry_request\ndata: {"requestId":"r"}\n\n',
      'event: update_available\ndata: {"version":"1.4","message":""}\n\n',
      'event: release_available\ndata: {"latestTag":"v9"}\n\n',
      'event: update_progress\ndata: {"phase":"verify"}\n\n',
      'event: data_corrupted\ndata: {}\n\n',
      'event: unknown_event\ndata: {"x":1}\n\n',
      'data: orphan-without-event\n\n',
      'random-line\n\n',
    ];
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce(makeSseResponse(events));
    const dispose = connectSseStream({});
    await new Promise((r) => setTimeout(r, 30));
    dispose();
    // maintenance(false) 应当把信号置为 false 分支
    expect(maintenanceActive()).toBe(false);
    fetchSpy.mockRestore();
  });

  it('rejects amas_state when payload is null/non-object via isAmasStatePayload', async () => {
    const events = [
      'event: amas_state\ndata: null\n\n',
      'event: amas_state\ndata: "string-payload"\n\n',
    ];
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce(makeSseResponse(events));
    const onAmasState = vi.fn();
    const dispose = connectSseStream({ onAmasState });
    await new Promise((r) => setTimeout(r, 30));
    dispose();
    expect(onAmasState).not.toHaveBeenCalled();
    fetchSpy.mockRestore();
  });

  it('rejects amas_state when callback missing and skips telemetry without requestId', async () => {
    const events = [
      'event: amas_state\ndata: {"attention":0.1,"fatigue":0.1,"motivation":0.1,"confidence":0.1,"sessionEventCount":1,"totalEventCount":1}\n\n',
      'event: telemetry_request\ndata: {}\n\n',
      'event: update_available\ndata: {}\n\n',
      'event: release_available\ndata: {"latestTag":123}\n\n',
      'event: update_progress\ndata: {"phase":123}\n\n',
    ];
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockResolvedValueOnce(makeSseResponse(events));
    const dispose = connectSseStream({});
    await new Promise((r) => setTimeout(r, 30));
    dispose();
    fetchSpy.mockRestore();
  });

  it('advances reconnectDelay (exponential backoff) when token present', async () => {
    // connectSseStream 是 admin-ui 专用连接，判断"是否还有 token"看的是 admin token 槽
    // （见 http.ts 的 bug 修复：此前误读了普通用户槽，导致这条 SSE 对任何 admin 会话恒 401）。
    tm._setAdmin('tok');
    vi.useFakeTimers();
    let calls = 0;
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockImplementation(async () => {
      calls += 1;
      return new Response('err', { status: 500 });
    });
    const dispose = connectSseStream({});
    // 让 startStream 进入 catch
    await Promise.resolve();
    await Promise.resolve();
    // 推进 4s 跨过 SSE_INITIAL_RECONNECT_MS=3000 触发第二轮 + 命中 reconnectDelay 加倍行
    await vi.advanceTimersByTimeAsync(4000);
    dispose();
    vi.useRealTimers();
    expect(calls).toBeGreaterThanOrEqual(2);
    fetchSpy.mockRestore();
  });
});
