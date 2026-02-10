import { tokenManager } from '@/lib/token';
import { createSignal } from 'solid-js';

export class ApiError extends Error {
  constructor(
    public status: number,
    public code: string,
    message: string,
    public traceId?: string,
  ) {
    super(message);
    this.name = 'ApiError';
  }
}

// ── Reactive 401 signal for SPA navigation (avoids hard refresh) ──
const [unauthorized, setUnauthorized] = createSignal(false);
export { unauthorized };

/** Reset unauthorized state (call after successful login) */
export function resetUnauthorized() {
  setUnauthorized(false);
}

async function unwrap<T>(response: Response): Promise<T> {
  if (!response.ok) {
    let body: Record<string, string> = {};
    try { body = await response.json(); } catch { /* not JSON */ }

    if (response.status === 401) {
      tokenManager.clearTokens();
      setUnauthorized(true);
    }

    throw new ApiError(
      response.status,
      body.code ?? 'UNKNOWN',
      body.message ?? body.error ?? response.statusText,
      body.traceId,
    );
  }

  if (response.status === 204 || response.headers.get('content-length') === '0') {
    return undefined as T;
  }
  const json = await response.json();
  if (json && typeof json === 'object' && 'success' in json) {
    if (json.success) return json.data as T;
    throw new ApiError(response.status, json.code ?? 'API_ERROR', json.message ?? json.error);
  }
  return json as T;
}

function buildUrl(path: string, params?: Record<string, string | number | boolean | undefined>): string {
  const url = new URL(path, window.location.origin);
  if (params) {
    for (const [key, value] of Object.entries(params)) {
      if (value !== undefined) url.searchParams.set(key, String(value));
    }
  }
  return url.toString();
}

interface ReqOpts extends RequestInit {
  params?: Record<string, string | number | boolean | undefined>;
  timeout?: number;
  useAdminToken?: boolean;
  /** Skip automatic token refresh check (used by the refresh endpoint itself) */
  skipTokenRefresh?: boolean;
  /** Override the Authorization header (e.g. for refresh token) */
  overrideAuth?: string;
}

// ── Token refresh lock to prevent concurrent refresh requests ──
let refreshPromise: Promise<void> | null = null;

async function ensureFreshToken(headers: Headers) {
  if (!tokenManager.needsRefresh()) return;

  // If a refresh is already in progress, wait for it
  if (refreshPromise) {
    await refreshPromise;
    const token = tokenManager.getToken();
    if (token) headers.set('Authorization', `Bearer ${token}`);
    return;
  }

  // Start a new refresh, other concurrent callers will await the same promise
  refreshPromise = (async () => {
    try {
      const { authApi } = await import('@/api/auth');
      const res = await authApi.refresh();
      tokenManager.setTokens(res.accessToken, res.refreshToken);
    } catch {
      // Refresh failed — clear tokens and trigger re-login
      tokenManager.clearTokens();
      setUnauthorized(true);
    }
  })();

  try {
    await refreshPromise;
    const token = tokenManager.getToken();
    if (token) headers.set('Authorization', `Bearer ${token}`);
  } finally {
    refreshPromise = null;
  }
}

async function req<T>(path: string, opts: ReqOpts = {}): Promise<T> {
  const { params, timeout = 30000, useAdminToken = false, skipTokenRefresh = false, overrideAuth, ...fetchOpts } = opts;
  const url = buildUrl(path, params);
  const headers = new Headers(fetchOpts.headers);

  if (!headers.has('Content-Type') && fetchOpts.body && typeof fetchOpts.body === 'string') {
    headers.set('Content-Type', 'application/json');
  }

  if (overrideAuth) {
    headers.set('Authorization', overrideAuth);
  } else {
    const token = useAdminToken ? tokenManager.getAdminToken() : tokenManager.getToken();
    if (token) headers.set('Authorization', `Bearer ${token}`);
  }

  const ctrl = new AbortController();
  const timer = setTimeout(() => ctrl.abort(), timeout);

  try {
    if (!useAdminToken && !skipTokenRefresh) {
      await ensureFreshToken(headers);
    }

    const response = await fetch(url, { ...fetchOpts, headers, signal: ctrl.signal });
    return await unwrap<T>(response);
  } finally {
    clearTimeout(timer);
  }
}

export const api = {
  get<T>(path: string, params?: Record<string, string | number | boolean | undefined>, opts?: ReqOpts) {
    return req<T>(path, { ...opts, method: 'GET', params });
  },
  post<T>(path: string, body?: unknown, opts?: ReqOpts) {
    // If caller already set opts.body (e.g. FormData), use it as-is
    if (opts?.body) {
      return req<T>(path, { ...opts, method: 'POST' });
    }
    return req<T>(path, {
      ...opts, method: 'POST',
      body: body ? JSON.stringify(body) : undefined,
      headers: { 'Content-Type': 'application/json', ...((opts?.headers as Record<string, string>) ?? {}) },
    });
  },
  put<T>(path: string, body?: unknown, opts?: ReqOpts) {
    if (opts?.body) {
      return req<T>(path, { ...opts, method: 'PUT' });
    }
    return req<T>(path, {
      ...opts, method: 'PUT',
      body: body ? JSON.stringify(body) : undefined,
      headers: { 'Content-Type': 'application/json', ...((opts?.headers as Record<string, string>) ?? {}) },
    });
  },
  delete<T>(path: string, opts?: ReqOpts) {
    return req<T>(path, { ...opts, method: 'DELETE' });
  },
};
