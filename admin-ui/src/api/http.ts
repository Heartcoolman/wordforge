import { tokenManager } from '@/lib/token';
import { getDeviceId, getDevicePlatform } from '@/lib/device';
import { ingestServerTimeFromResponse } from '@/lib/clockSkew';
import { createSignal } from 'solid-js';
import type { AmasStateStreamEvent } from '@/types/amas';

export type { AmasStateStreamEvent };

const API_BASE_URL = (import.meta.env.VITE_API_BASE_URL as string | undefined)?.trim();

const DEFAULT_TIMEOUT_MS = 30_000;
const SSE_INITIAL_RECONNECT_MS = 3_000;
const SSE_MAX_RECONNECT_MS = 30_000;
const SSE_READ_TIMEOUT_MS = 60_000;
// 连接存活超过该时长才视为「健康连接」，重连退避才重置；否则按指数退避，
// 避免代理 idle-close / 服务端干净结束流时形成零延迟重连风暴。
const SSE_STABLE_UPTIME_MS = 30_000;

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

// ── Maintenance mode reactive signal ──
const [maintenanceActive, setMaintenanceActive] = createSignal(false);
export { maintenanceActive, setMaintenanceActive };

// ── Update info reactive signal ──
const [updateInfo, setUpdateInfo] = createSignal<{ version: string; message: string } | null>(null);
export { updateInfo, setUpdateInfo };

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
  // 空 body 但无 Content-Length:0(chunked / 代理剥离长度头)时,response.json() 会抛
  // SyntaxError 逃逸统一错误处理。先读文本判空,空则返回 undefined,非空再解析。
  const text = await response.text();
  if (!text) {
    return undefined as unknown as T;
  }
  const json = JSON.parse(text);
  if (json && typeof json === 'object' && 'success' in json) {
    if (json.success) return json.data as T;
    throw new ApiError(response.status, json.code ?? 'API_ERROR', json.message ?? json.error);
  }
  return json as T;
}

export function buildUrl(path: string, params?: Record<string, string | number | boolean | undefined>): string {
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
  headers.set('X-Device-Id', getDeviceId());
  headers.set('X-Device-Platform', getDevicePlatform());

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
    // 每个响应都带 X-Server-Time，第一时间更新时钟偏移，保证后续 isTokenExpired 用对的基准
    ingestServerTimeFromResponse(response);

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
          ingestServerTimeFromResponse(response);
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

export interface SseCallbacks {
  onAmasState?: (payload: AmasStateStreamEvent) => void;
  onMaintenance?: (active: boolean) => void;
  onTelemetryRequest?: (requestId: string) => void;
  onUpdateAvailable?: (payload: { version: string; message: string }) => void;
  /** 后端 `release_available`：探测到 GitHub Releases 有新二进制版本（仅 admin 关心）。
   * v0.6.0-beta.3：payload 含 `channel`（stable / beta），admin UI 据此在对应通道卡片亮 badge。
   * 旧后端无 channel 字段时 fallback 当 stable，保持渐进迁移。 */
  onReleaseAvailable?: (payload: { latestTag: string; channel: 'stable' | 'beta' }) => void;
  /** 后端 `update_progress`：一键自更新执行进度（仅 admin 关心） */
  onUpdateProgress?: (payload: { phase: string; percent: number }) => void;
  /** 后端 `new_llm_suggestion`：LLM 调参建议生成（仅 admin 关心，advisor 页用） */
  onNewLlmSuggestion?: (payload: { suggestionId: number }) => void;
  /** 后端 `probe_request`：admin 远程探针下发，Worker 沙箱里执行 base64 解码后的 script */
  onProbeRequest?: (payload: {
    requestId: string;
    batchId: string;
    scriptB64: string;
    timeoutMs: number;
    ctxVersion: number;
  }) => void;
  /** 后端 `probe_confirm`：D 类受控写二次确认通过，客户端用同一 ctx 快照重跑 */
  onProbeConfirm?: (payload: { requestId: string; confirmToken: string }) => void;
  onDataCorrupted?: () => void;
  /** 后端 `incident`：滚动 5 分钟内 5xx 错误率超阈值（同窗口 5 分钟内 dedup） */
  onIncident?: (payload: { errorRate: number; windowSecs: number }) => void;
  /** 后端 `worker_missed`：worker 连续 3 个调度周期未上报，调度器健康告警 */
  onWorkerMissed?: (payload: { workerName: string; missCount: number }) => void;
  /** 后端 `llm_budget_exceeded`：LLM advisor 月度人民币成本超上限，当月 worker 已自动停跑 */
  onLlmBudgetExceeded?: (payload: { spentYuan: number; capYuan: number; resumeMonth: string }) => void;
}

export function connectSseStream(callbacks: SseCallbacks): () => void {
  let aborted = false;
  let currentCtrl: AbortController | null = null;
  let reconnectDelay = SSE_INITIAL_RECONNECT_MS;

  async function startStream() {
    while (!aborted) {
      const ctrl = new AbortController();
      currentCtrl = ctrl;
      try {
        // 注：needsRefresh()/refreshAccessToken() 检查的是普通用户 token 槽（管理员登录只写
        // getAdminToken() 那一槽，见 lib/token.ts），对 admin 会话恒是 no-op；管理员 token 目前
        // 没有静默刷新机制，过期后需要重新登录。此处保留原判断不动，避免引入未经设计的新刷新流程。
        if (tokenManager.needsRefresh()) {
          await tokenManager.refreshAccessToken();
        }

        // Bug 修复：这条 SSE 连接专供 admin-ui 使用，此前一直读普通用户 token 槽
        // （tokenManager.getToken()）——管理员登录只调用 setAdminToken()，从不写这一槽，导致
        // 这条连接对每一个 admin 会话都永远拿不到 token、必然 401。App.tsx 里"admin 登录后会
        // 自然恢复"的注释此前并不成立（因为这里从没读对过槽位），改读 getAdminToken() 后才真正
        // 兑现该注释描述的行为。
        const token = tokenManager.getAdminToken();
        const response = await fetch(buildUrl('/api/realtime/events'), {
          headers: {
            ...(token ? { 'Authorization': `Bearer ${token}` } : {}),
            'Accept': 'text/event-stream',
            'X-Device-Id': getDeviceId(),
            'X-Device-Platform': getDevicePlatform(),
          },
          credentials: 'include',
          signal: ctrl.signal,
        });

        if (!response.ok || !response.body) {
          throw new Error(`SSE 连接失败: ${response.status}`);
        }

        const connectedAt = Date.now();

        const reader = response.body.getReader();
        const decoder = new TextDecoder();
        let buffer = '';
        let eventType = '';

        while (!aborted) {
          let timerId: ReturnType<typeof setTimeout> | undefined;
          const timeout = new Promise<{ done: true; value: undefined }>((resolve) => {
            timerId = setTimeout(() => resolve({ done: true, value: undefined }), SSE_READ_TIMEOUT_MS);
          });
          let result;
          try {
            result = await Promise.race([
              reader.read().catch(() => ({ done: true as const, value: undefined })),
              timeout,
            ]);
          } finally {
            clearTimeout(timerId);
          }
          if (result.done) { reader.cancel(); break; }

          buffer += decoder.decode(result.value, { stream: true });
          const lines = buffer.split('\n');
          buffer = lines.pop() ?? '';

          for (const line of lines) {
            if (line.startsWith('event:')) {
              eventType = line.slice(6).trim();
            } else if (line.startsWith('data:') && eventType) {
              try {
                const data = JSON.parse(line.slice(5).trim());
                if (eventType === 'amas_state' && callbacks.onAmasState && isAmasStatePayload(data)) {
                  callbacks.onAmasState(data);
                } else if (eventType === 'maintenance') {
                  setMaintenanceActive(!!data.active);
                  callbacks.onMaintenance?.(data.active);
                } else if (eventType === 'telemetry_request' && callbacks.onTelemetryRequest && data.requestId) {
                  callbacks.onTelemetryRequest(data.requestId);
                } else if (eventType === 'update_available' && data.version) {
                  setUpdateInfo({ version: data.version, message: data.message || '' });
                  callbacks.onUpdateAvailable?.({ version: data.version, message: data.message || '' });
                } else if (eventType === 'release_available' && typeof data.latestTag === 'string') {
                  const channel: 'stable' | 'beta' = data.channel === 'beta' ? 'beta' : 'stable';
                  callbacks.onReleaseAvailable?.({ latestTag: data.latestTag, channel });
                } else if (eventType === 'update_progress' && typeof data.phase === 'string') {
                  callbacks.onUpdateProgress?.({
                    phase: data.phase,
                    percent: Number(data.percent) || 0,
                  });
                } else if (eventType === 'new_llm_suggestion' && typeof data.suggestionId === 'number') {
                  callbacks.onNewLlmSuggestion?.({ suggestionId: data.suggestionId });
                } else if (
                  eventType === 'probe_request'
                  && callbacks.onProbeRequest
                  && typeof data.requestId === 'string'
                  && typeof data.batchId === 'string'
                  && typeof data.scriptB64 === 'string'
                ) {
                  callbacks.onProbeRequest({
                    requestId: data.requestId,
                    batchId: data.batchId,
                    scriptB64: data.scriptB64,
                    timeoutMs: Number(data.timeoutMs) || 3000,
                    ctxVersion: Number(data.ctxVersion) || 1,
                  });
                } else if (
                  eventType === 'probe_confirm'
                  && callbacks.onProbeConfirm
                  && typeof data.requestId === 'string'
                  && typeof data.confirmToken === 'string'
                ) {
                  callbacks.onProbeConfirm({
                    requestId: data.requestId,
                    confirmToken: data.confirmToken,
                  });
                } else if (eventType === 'data_corrupted') {
                  callbacks.onDataCorrupted?.();
                } else if (
                  eventType === 'incident'
                  && typeof data.errorRate === 'number'
                  && typeof data.windowSecs === 'number'
                ) {
                  callbacks.onIncident?.({ errorRate: data.errorRate, windowSecs: data.windowSecs });
                } else if (
                  eventType === 'worker_missed'
                  && typeof data.workerName === 'string'
                  && typeof data.missCount === 'number'
                ) {
                  callbacks.onWorkerMissed?.({ workerName: data.workerName, missCount: data.missCount });
                } else if (
                  eventType === 'llm_budget_exceeded'
                  && typeof data.spentYuan === 'number'
                  && typeof data.capYuan === 'number'
                  && typeof data.resumeMonth === 'string'
                ) {
                  callbacks.onLlmBudgetExceeded?.({
                    spentYuan: data.spentYuan,
                    capYuan: data.capYuan,
                    resumeMonth: data.resumeMonth,
                  });
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

        // 内层循环正常结束（流被干净关闭 / 读超时 done=true）也要退避，否则会零延迟重连风暴。
        if (aborted) return;
        if (Date.now() - connectedAt >= SSE_STABLE_UPTIME_MS) {
          // 长存活连接：视为健康，重置退避。
          reconnectDelay = SSE_INITIAL_RECONNECT_MS;
        }
        await new Promise(resolve => setTimeout(resolve, reconnectDelay));
        reconnectDelay = Math.min(reconnectDelay * 2, SSE_MAX_RECONNECT_MS);
      } catch (err) {
        if (aborted) return;
        // 同上：判断"是否还有 token 可重连"要看的是 admin token 槽，不是普通用户槽。
        const delay = !tokenManager.getAdminToken() ? SSE_MAX_RECONNECT_MS : reconnectDelay;
        await new Promise(resolve => setTimeout(resolve, delay));
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

export function connectAmasStateStream(
  onState: (payload: AmasStateStreamEvent) => void,
): () => void {
  return connectSseStream({ onAmasState: onState });
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
  patch<T>(path: string, body?: unknown, opts?: ReqOpts) {
    if (opts?.body) {
      return req<T>(path, { ...opts, method: 'PATCH' });
    }
    return req<T>(path, {
      ...opts, method: 'PATCH',
      body: body ? JSON.stringify(body) : undefined,
    });
  },
  delete<T>(path: string, opts?: ReqOpts) {
    return req<T>(path, { ...opts, method: 'DELETE' });
  },
};
