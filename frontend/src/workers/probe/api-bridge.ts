/**
 * 主线程编排器：
 *   1. 通过 `connectSseStream` 监听 `probe_request` 事件；
 *   2. 异步采集 ctx 快照（M2 起完整 v1 schema，含 idb 200ms cap）；
 *   3. spawn dedicated Worker 跑 admin 下发的 script，timeout_ms 强制 terminate；
 *   4. JSON 序列化 result，超 256KB 截断 + truncated=true；
 *   5. `POST /api/probe/results` 回传。
 */

import { api, connectSseStream } from '@/api/client';
import { collectCtxSnapshot } from './ctx-factory';
import { CLIENT_CTX_VERSION, type ResultPayload, type WorkerInput, type WorkerOutput } from './types';

const RESULT_BYTES_LIMIT = 256 * 1024;
const HARD_KILL_GUARD_MS = 500; // 在 script 超时 + 此 buffer 后仍未回来 → terminate

let stopSseStream: (() => void) | undefined;

export function startProbeBridge(): void {
  if (stopSseStream) return;
  stopSseStream = connectSseStream({
    onProbeRequest: handleProbeRequest,
  });
}

export function stopProbeBridge(): void {
  stopSseStream?.();
  stopSseStream = undefined;
}

async function handleProbeRequest(payload: {
  requestId: string;
  batchId: string;
  scriptB64: string;
  timeoutMs: number;
  ctxVersion: number;
}): Promise<void> {
  // ── 0. ctx_version 协商 ──
  if (payload.ctxVersion !== CLIENT_CTX_VERSION) {
    await postResult({
      requestId: payload.requestId,
      status: 'unsupported_ctx_version',
      stderr: `client ctx v${CLIENT_CTX_VERSION}, server requested v${payload.ctxVersion}`,
      durationMs: 0,
      truncated: false,
    });
    return;
  }

  // ── 1. 采集 ctx 快照 ──
  const snapshot = await collectCtxSnapshot();

  // ── 2. 解码 script ──
  let script: string;
  try {
    script = atob(payload.scriptB64);
  } catch (err) {
    await postResult({
      requestId: payload.requestId,
      status: 'error',
      stderr: `script base64 decode failed: ${String(err)}`,
      durationMs: 0,
      truncated: false,
    });
    return;
  }

  // ── 3. spawn Worker + race timeout ──
  const worker = new Worker(new URL('./runner.worker.ts', import.meta.url), { type: 'module' });
  const start = performance.now();
  const timeoutMs = payload.timeoutMs;

  const finalize = (result: ResultPayload) => {
    try {
      worker.terminate();
    } catch {
      /* ignore */
    }
    void postResult(result);
  };

  let settled = false;
  const onResult = (out: WorkerOutput) => {
    if (settled) return;
    settled = true;
    if (out.ok) {
      const { json, truncated } = serializeAndTruncate(out.result);
      finalize({
        requestId: payload.requestId,
        status: 'ok',
        resultJson: truncated ? { _truncated_raw: json } : safeJsonParse(json),
        durationMs: out.durationMs,
        truncated,
      });
    } else {
      finalize({
        requestId: payload.requestId,
        status: 'error',
        stderr: out.stderr,
        durationMs: out.durationMs,
        truncated: false,
      });
    }
  };

  worker.onmessage = (ev: MessageEvent<WorkerOutput>) => onResult(ev.data);
  worker.onerror = (ev) => {
    if (settled) return;
    settled = true;
    finalize({
      requestId: payload.requestId,
      status: 'error',
      stderr: ev.message || 'worker error',
      durationMs: Math.round(performance.now() - start),
      truncated: false,
    });
  };

  setTimeout(() => {
    if (settled) return;
    settled = true;
    finalize({
      requestId: payload.requestId,
      status: 'timeout',
      durationMs: timeoutMs,
      truncated: false,
    });
  }, timeoutMs + HARD_KILL_GUARD_MS);

  const input: WorkerInput = { script, snapshot };
  worker.postMessage(input);
}

export function serializeAndTruncate(value: unknown): { json: string; truncated: boolean } {
  let json: string | undefined;
  try {
    json = JSON.stringify(value);
  } catch (err) {
    return { json: `[unserializable: ${String(err)}]`, truncated: false };
  }
  // JSON.stringify(undefined) 返回 undefined（非字符串）→ 当作不可序列化处理。
  if (typeof json !== 'string') {
    return { json: '[unserializable: undefined or fn]', truncated: false };
  }
  if (json.length > RESULT_BYTES_LIMIT) {
    return { json: json.slice(0, RESULT_BYTES_LIMIT), truncated: true };
  }
  return { json, truncated: false };
}

function safeJsonParse(json: string): unknown {
  try {
    return JSON.parse(json);
  } catch {
    return json;
  }
}

async function postResult(body: ResultPayload): Promise<void> {
  try {
    await api.post('/api/probe/results', body);
  } catch (err) {
    // 失败不重试 —— admin 端会看到对应 device 卡片始终 pending；后续 M3+
    // 可加重试逻辑，M2 阶段记录到 console 即可。
    // eslint-disable-next-line no-console
    console.warn('[probe] postResult failed', err);
  }
}
