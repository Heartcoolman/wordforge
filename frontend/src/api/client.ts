import { tokenManager } from '@/lib/token';
import { createSignal } from 'solid-js';
import type { AmasStateStreamEvent } from '@/types/amas';

export type { AmasStateStreamEvent };

const API_BASE_URL = (import.meta.env.VITE_API_BASE_URL as string | undefined)?.trim();

const DEFAULT_TIMEOUT_MS = 30_000;
const SSE_INITIAL_RECONNECT_MS = 3_000;
const SSE_MAX_RECONNECT_MS = 30_000;

function resolveApiBase(): string {
  if (!API_BASE_URL) return window.location.origin;
  try {
    return new URL(API_BASE_URL, window.location.origin).toString();
  } catch {
    return window.location.origin;
  }
}

const API_BASE = resolveApiBase();

export class ApiError extends Error {
  constructor(
    public status: number,
    public code: string,
    message: string,
    public traceId?: string,
    public retryAfter?: number,
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

async function unwrap<T>(response: Response, context?: { useAdminToken: boolean }): Promise<T> {
  if (!response.ok) {
    let body: Record<string, string> = {};
    try { body = await response.json(); } catch { /* not JSON */ }

    if (response.status === 401) {
      if (context?.useAdminToken) {
        tokenManager.clearAdminToken();
        window.dispatchEvent(new Event('admin:unauthorized'));
      } else {
        // 完整清理本地状态（不发 API 请求，避免递归）
        tokenManager.clearTokens();
        setUnauthorized(true);
      }
    }

    if (response.status === 429) {
      const retryAfterHeader = response.headers.get('Retry-After');
      const retryAfter = retryAfterHeader ? parseInt(retryAfterHeader, 10) : undefined;
      const message = retryAfter
        ? `请求过于频繁，请在 ${retryAfter} 秒后重试`
        : '请求过于频繁，请稍后重试';
      const err = new ApiError(429, body.code ?? 'RATE_LIMITED', message, body.traceId, retryAfter);
      throw err;
    }

    throw new ApiError(
      response.status,
      body.code ?? 'UNKNOWN',
      body.message ?? body.error ?? response.statusText,
      body.traceId,
    );
  }

  if (response.status === 204 || response.headers.get('content-length') === '0') {
    return undefined as unknown as T;
  }
  const json = await response.json();
  if (json && typeof json === 'object' && 'success' in json) {
    if (json.success) return json.data as T;
    throw new ApiError(response.status, json.code ?? 'API_ERROR', json.message ?? json.error);
  }
  return json as T;
}

function buildUrl(path: string, params?: Record<string, string | number | boolean | undefined>): string {
  const normalizedPath = path.startsWith('/') ? path : `/${path}`;
  const url = new URL(normalizedPath, API_BASE);
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
}

function setAuthorizationHeader(headers: Headers, useAdminToken: boolean): void {
  const token = useAdminToken ? tokenManager.getAdminToken() : tokenManager.getToken();
  if (token) {
    headers.set('Authorization', `Bearer ${token}`);
    return;
  }
  headers.delete('Authorization');
}

function canRetryUnauthorized(path: string, useAdminToken: boolean, skipTokenRefresh: boolean): boolean {
  if (useAdminToken || skipTokenRefresh) {
    return false;
  }
  // Public auth endpoints should return auth errors directly.
  if (path.startsWith('/api/auth/')) {
    return false;
  }
  return true;
}

async function req<T>(path: string, opts: ReqOpts = {}): Promise<T> {
  const { params, timeout = DEFAULT_TIMEOUT_MS, useAdminToken = false, skipTokenRefresh = false, ...fetchOpts } = opts;
  const url = buildUrl(path, params);
  const headers = new Headers(fetchOpts.headers);

  if (!headers.has('Content-Type') && fetchOpts.body && typeof fetchOpts.body === 'string') {
    headers.set('Content-Type', 'application/json');
  }

  setAuthorizationHeader(headers, useAdminToken);

  const ctrl = new AbortController();
  const timer = setTimeout(() => ctrl.abort(), timeout);
  const credentials = fetchOpts.credentials ?? 'include';

  try {
    if (!useAdminToken && !skipTokenRefresh) {
      if (tokenManager.needsRefresh()) {
        const refreshed = await tokenManager.refreshAccessToken();
        if (refreshed) {
          setAuthorizationHeader(headers, false);
        }
      }
    }

    let response = await fetch(url, {
      ...fetchOpts,
      credentials,
      headers,
      signal: ctrl.signal,
    });

    if (response.status === 401 && canRetryUnauthorized(path, useAdminToken, skipTokenRefresh)) {
      const refreshed = await tokenManager.refreshAccessToken();
      if (refreshed) {
        const retryHeaders = new Headers(headers);
        setAuthorizationHeader(retryHeaders, false);
        // 创建新的 AbortController，因为原始的可能已被中止
        const retryCtrl = new AbortController();
        const retryTimer = setTimeout(() => retryCtrl.abort(), timeout);
        try {
          response = await fetch(url, {
            ...fetchOpts,
            credentials,
            headers: retryHeaders,
            signal: retryCtrl.signal,
          });
        } finally {
          clearTimeout(retryTimer);
        }
      } else {
        setUnauthorized(true);
      }
    }

    return await unwrap<T>(response, { useAdminToken });
  } catch (err) {
    if (err instanceof ApiError) throw err;
    if (err instanceof DOMException && err.name === 'AbortError') {
      throw new ApiError(0, 'TIMEOUT', '请求超时，请稍后重试');
    }
    if (err instanceof TypeError) {
      throw new ApiError(0, 'NETWORK_ERROR', '网络连接失败，请检查网络');
    }
    throw err;
  } finally {
    clearTimeout(timer);
  }
}

function isAmasStatePayload(payload: unknown): payload is AmasStateStreamEvent {
  if (!payload || typeof payload !== 'object') return false;
  const record = payload as Record<string, unknown>;
  return typeof record.attention === 'number'
    && typeof record.fatigue === 'number'
    && typeof record.motivation === 'number'
    && typeof record.confidence === 'number'
    && typeof record.sessionEventCount === 'number'
    && typeof record.totalEventCount === 'number';
}

export function connectAmasStateStream(
  onState: (payload: AmasStateStreamEvent) => void,
): () => void {
  let aborted = false;
  let currentCtrl: AbortController | null = null;
  let reconnectDelay = SSE_INITIAL_RECONNECT_MS;

  async function startStream() {
    while (!aborted) {
      const ctrl = new AbortController();
      currentCtrl = ctrl;
      try {
        // SSE 连接前检查 token 是否需要刷新
        if (tokenManager.needsRefresh()) {
          await tokenManager.refreshAccessToken();
        }

        const token = tokenManager.getToken();
        const response = await fetch(buildUrl('/api/realtime/events'), {
          headers: {
            ...(token ? { 'Authorization': `Bearer ${token}` } : {}),
            'Accept': 'text/event-stream',
          },
          credentials: 'include',
          signal: ctrl.signal,
        });

        if (!response.ok || !response.body) {
          throw new Error(`SSE 连接失败: ${response.status}`);
        }

        reconnectDelay = SSE_INITIAL_RECONNECT_MS;

        const reader = response.body.getReader();
        const decoder = new TextDecoder();
        let buffer = '';
        let eventType = '';

        while (!aborted) {
          const { done, value } = await reader.read();
          if (done) break;

          buffer += decoder.decode(value, { stream: true });
          const lines = buffer.split('\n');
          buffer = lines.pop() ?? '';

          for (const line of lines) {
            if (line.startsWith('event:')) {
              eventType = line.slice(6).trim();
            } else if (line.startsWith('data:') && eventType === 'amas_state') {
              try {
                const payload = JSON.parse(line.slice(5).trim()) as unknown;
                if (isAmasStatePayload(payload)) {
                  onState(payload);
                }
              } catch {
                // 忽略格式错误的事件数据
              }
              eventType = '';
            } else if (line === '') {
              eventType = '';
            }
          }
        }
      } catch (err) {
        if (aborted) return;
        await new Promise(resolve => setTimeout(resolve, reconnectDelay));
        reconnectDelay = Math.min(reconnectDelay * 2, SSE_MAX_RECONNECT_MS);
      }
    }
  }

  startStream();

  return () => {
    aborted = true;
    currentCtrl?.abort();
  };
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
    });
  },
  put<T>(path: string, body?: unknown, opts?: ReqOpts) {
    if (opts?.body) {
      return req<T>(path, { ...opts, method: 'PUT' });
    }
    return req<T>(path, {
      ...opts, method: 'PUT',
      body: body ? JSON.stringify(body) : undefined,
    });
  },
  delete<T>(path: string, opts?: ReqOpts) {
    return req<T>(path, { ...opts, method: 'DELETE' });
  },
};
