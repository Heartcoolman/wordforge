/**
 * 主线程编排器：
 *   1. 通过 `connectSseStream` 监听 `probe_request` / `probe_confirm` 事件；
 *   2. 异步采集 ctx 快照（M2 起完整 v1 schema，含 idb 200ms cap）；
 *   3. spawn dedicated Worker 跑 admin 下发的 script，timeout_ms 强制 terminate；
 *   4. 若 script 调了 ctx.cmd.* → 推到 _actions → 首次回 confirm_required，缓存
 *      ctx_snapshot + script + actions 等 admin 确认（TTL 60s）；
 *   5. 收到 ProbeConfirm → 用缓存的 snapshot 重跑，执行 actions（序固化
 *      clearCache → signOut → reload，signOut 后中断）；
 *   6. JSON 序列化 result，超 256KB 截断 + truncated=true；
 *   7. `POST /api/probe/results` 回传。
 */

import { api, connectSseStream } from '@/api/client';
import { collectCtxSnapshot } from './ctx-factory';
import {
  CLIENT_CTX_VERSION,
  type CtxSnapshot,
  type ProbeAction,
  type ResultPayload,
  type WorkerInput,
  type WorkerOutput,
} from './types';

const RESULT_BYTES_LIMIT = 256 * 1024;
const HARD_KILL_GUARD_MS = 500;
const CONFIRM_CACHE_TTL_MS = 60_000;

interface ConfirmCacheEntry {
  script: string;
  snapshot: CtxSnapshot;
  batchId: string;
  timeoutMs: number;
  confirmToken: string;
  expiresAt: number;
}

/** request_id → 重跑所需上下文。TTL 60s 与后端 CONFIRM_TTL_SECS 一致。 */
const pendingConfirms = new Map<string, ConfirmCacheEntry>();

let stopSseStream: (() => void) | undefined;

export function startProbeBridge(): void {
  if (stopSseStream) return;
  stopSseStream = connectSseStream({
    onProbeRequest: handleProbeRequest,
    onProbeConfirm: handleProbeConfirm,
  });
}

export function stopProbeBridge(): void {
  stopSseStream?.();
  stopSseStream = undefined;
  pendingConfirms.clear();
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

  await runScript(payload.requestId, script, snapshot, payload.timeoutMs, payload.batchId, false);
}

async function handleProbeConfirm(payload: {
  requestId: string;
  confirmToken: string;
}): Promise<void> {
  const cached = pendingConfirms.get(payload.requestId);
  if (!cached) {
    await postResult({
      requestId: payload.requestId,
      status: 'error',
      stderr: 'confirm cache miss（TTL 已过或客户端刷新）',
      durationMs: 0,
      truncated: false,
    });
    return;
  }
  if (cached.confirmToken !== payload.confirmToken) {
    await postResult({
      requestId: payload.requestId,
      status: 'error',
      stderr: 'confirm token 不匹配',
      durationMs: 0,
      truncated: false,
    });
    return;
  }
  pendingConfirms.delete(payload.requestId);
  await runScript(payload.requestId, cached.script, cached.snapshot, cached.timeoutMs, cached.batchId, true);
}

async function runScript(
  requestId: string,
  script: string,
  snapshot: CtxSnapshot,
  timeoutMs: number,
  batchId: string,
  confirmed: boolean,
): Promise<void> {
  const worker = new Worker(new URL('./runner.worker.ts', import.meta.url), { type: 'module' });
  const start = performance.now();

  const finalize = (result: ResultPayload) => {
    try {
      worker.terminate();
    } catch {
      /* ignore */
    }
    void postResult(result);
  };

  let settled = false;
  const onResult = async (out: WorkerOutput) => {
    if (settled) return;
    settled = true;
    if (!out.ok) {
      finalize({
        requestId,
        status: 'error',
        stderr: out.stderr,
        durationMs: out.durationMs,
        truncated: false,
      });
      return;
    }

    // 检测 D 类 cmd actions：未确认且 actions 非空 → 走 confirm_required 流
    if (!confirmed && out.actions.length > 0) {
      const confirmToken = generateConfirmToken();
      pendingConfirms.set(requestId, {
        script,
        snapshot,
        batchId,
        timeoutMs,
        confirmToken,
        expiresAt: Date.now() + CONFIRM_CACHE_TTL_MS,
      });
      // TTL 自删
      setTimeout(() => {
        const entry = pendingConfirms.get(requestId);
        if (entry && entry.expiresAt <= Date.now()) pendingConfirms.delete(requestId);
      }, CONFIRM_CACHE_TTL_MS + 100);
      try {
        worker.terminate();
      } catch { /* ignore */ }
      await postResult({
        requestId,
        status: 'confirm_required',
        resultJson: { _actions: out.actions, _preview: out.result },
        durationMs: out.durationMs,
        truncated: false,
        confirmToken,
      });
      return;
    }

    // 截断 + 序列化
    const { json, truncated } = serializeAndTruncate(out.result);
    const resultJson = truncated ? { _truncated_raw: json } : safeJsonParse(json);

    // 已确认且有 actions → 主线程执行
    if (confirmed && out.actions.length > 0) {
      // 先 POST result（让 admin 看到 ok 状态），再异步执行 actions
      void postResult({
        requestId,
        status: 'ok',
        resultJson,
        durationMs: out.durationMs,
        truncated,
      });
      try {
        worker.terminate();
      } catch { /* ignore */ }
      // 序固化：clearCache → signOut → reload（signOut 后跳转，reload 跳过）
      await executeActions(out.actions);
      return;
    }

    finalize({
      requestId,
      status: 'ok',
      resultJson,
      durationMs: out.durationMs,
      truncated,
    });
  };

  worker.onmessage = (ev: MessageEvent<WorkerOutput>) => void onResult(ev.data);
  worker.onerror = (ev) => {
    if (settled) return;
    settled = true;
    finalize({
      requestId,
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
      requestId,
      status: 'timeout',
      durationMs: timeoutMs,
      truncated: false,
    });
  }, timeoutMs + HARD_KILL_GUARD_MS);

  const input: WorkerInput = { script, snapshot, confirmed };
  worker.postMessage(input);
}

/**
 * 按固化序执行 D 类 cmd action。signOut 会跳转 /login，跳转后 reload 无意义
 * → 短路返回。clearCache 清 localStorage + sessionStorage + Cache Storage
 * （不动 IndexedDB —— 避免误删本地学习数据）。
 */
export async function executeActions(actions: ProbeAction[]): Promise<void> {
  const order: Array<ProbeAction['type']> = ['clearCache', 'signOut', 'reload'];
  for (const type of order) {
    if (!actions.some((a) => a.type === type)) continue;
    if (type === 'clearCache') {
      try {
        localStorage.clear();
        sessionStorage.clear();
        if (typeof caches !== 'undefined' && caches.keys) {
          const keys = await caches.keys();
          await Promise.all(keys.map((k) => caches.delete(k)));
        }
      } catch (err) {
        // eslint-disable-next-line no-console
        console.warn('[probe] clearCache failed', err);
      }
    } else if (type === 'signOut') {
      // 清 token + redirect /login；后续 reload 短路
      try {
        const { tokenManager } = await import('@/lib/token');
        tokenManager.clearTokens();
      } catch { /* ignore */ }
      try {
        window.location.assign('/admin/login');
      } catch { /* ignore */ }
      return; // 短路
    } else if (type === 'reload') {
      try {
        window.location.reload();
      } catch { /* ignore */ }
    }
  }
}

function generateConfirmToken(): string {
  if (typeof crypto !== 'undefined' && 'randomUUID' in crypto) {
    return crypto.randomUUID();
  }
  // 降级：时间戳 + Math.random 拼接
  return `${Date.now()}-${Math.random().toString(36).slice(2)}`;
}

export function serializeAndTruncate(value: unknown): { json: string; truncated: boolean } {
  let json: string | undefined;
  try {
    json = JSON.stringify(value);
  } catch (err) {
    return { json: `[unserializable: ${String(err)}]`, truncated: false };
  }
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
    // eslint-disable-next-line no-console
    console.warn('[probe] postResult failed', err);
  }
}

/** 测试用：检查 pending confirm 缓存（不导出给 production）。 */
export function _testGetPendingConfirms() {
  return pendingConfirms;
}
