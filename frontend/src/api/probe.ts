/**
 * 远程探针 API client（admin 调用 + 普通用户 client 回传共用）。
 *
 * 与 `src/routes/admin/probe.rs` + `src/routes/probe_results.rs` 对齐。
 */

import { api, buildUrl } from '@/api/client';
import { tokenManager } from '@/lib/token';

export interface DispatchRequest {
  targets: {
    deviceIds?: string[];
    allOnline?: boolean;
  };
  script: string;
  timeoutMs?: number;
  note?: string;
}

export interface DispatchResponse {
  batchId: string;
  dispatched: Array<{ deviceId: string; requestId: string }>;
  skippedOffline: string[];
}

/** SSE 流推回来的单条 result（与 services::probe::ProbeResultPayload 对齐）。 */
export interface ProbeResultEvent {
  deviceId: string;
  requestId: string;
  batchId: string;
  status: string;
  resultJson?: unknown;
  stderr?: string;
  durationMs?: number;
  truncated?: boolean;
}

export const probeApi = {
  dispatch: (body: DispatchRequest) =>
    api.post<DispatchResponse>('/api/admin/probe', body, { useAdminToken: true }),
  /** D 类受控写二次确认：admin 仅输入 device_id 后 5 位。confirm_token 是
   *  客户端 ↔ 服务端的内部凭证，admin 不持有不需要。 */
  confirm: (requestId: string, body: { deviceIdSuffix: string }) =>
    api.post<{ confirmed: boolean }>(
      `/api/admin/probe/${encodeURIComponent(requestId)}/confirm`,
      body,
      { useAdminToken: true },
    ),
};

/**
 * 订阅 admin batch SSE 流。返回 stop fn。
 * SSE 在 fetch 失败 / 4xx / abort 时不会自动重连——admin 页面手动按需重启即可。
 */
export function connectProbeBatchStream(
  batchId: string,
  callbacks: {
    onResult?: (payload: ProbeResultEvent) => void;
    onCompleted?: (payload: { received: number; expected: number }) => void;
    onError?: (err: unknown) => void;
  },
): () => void {
  const ctrl = new AbortController();
  void runStream();
  return () => ctrl.abort();

  async function runStream() {
    try {
      const token = tokenManager.getAdminToken();
      const response = await fetch(
        buildUrl(`/api/admin/probe/${encodeURIComponent(batchId)}/stream`),
        {
          headers: {
            ...(token ? { Authorization: `Bearer ${token}` } : {}),
            Accept: 'text/event-stream',
          },
          credentials: 'include',
          signal: ctrl.signal,
        },
      );
      if (!response.ok || !response.body) {
        callbacks.onError?.(new Error(`probe stream HTTP ${response.status}`));
        return;
      }
      const reader = response.body.getReader();
      const decoder = new TextDecoder();
      let buffer = '';
      let eventType: string | null = null;
      while (!ctrl.signal.aborted) {
        const { value, done } = await reader.read();
        if (done) break;
        buffer += decoder.decode(value, { stream: true });
        const lines = buffer.split('\n');
        buffer = lines.pop() ?? '';
        for (const line of lines) {
          if (line.startsWith('event:')) {
            eventType = line.slice(6).trim();
          } else if (line.startsWith('data:') && eventType) {
            const dataStr = line.slice(5).trim();
            try {
              const data = JSON.parse(dataStr);
              if (eventType === 'result') {
                callbacks.onResult?.(data as ProbeResultEvent);
              } else if (eventType === 'completed') {
                callbacks.onCompleted?.(data);
              }
            } catch {
              /* 忽略格式错误的事件 */
            }
          } else if (line === '') {
            eventType = null;
          }
        }
      }
    } catch (err) {
      if (!ctrl.signal.aborted) callbacks.onError?.(err);
    }
  }
}
