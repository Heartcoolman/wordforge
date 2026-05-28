/// <reference lib="webworker" />
/**
 * 远程探针沙箱 Worker —— 真正 eval admin 下发 script 的地方。
 *
 * 边界：Dedicated Worker 默认无 DOM / document / window；fetch 在 Worker
 * 全局存在，但 ctx 不暴露给 script。Worker 与主线程仅通过 postMessage 通信，
 * script 拿到的 ctx 是一次性快照值（无 live 副作用）。
 *
 * ctx 构造：主线程传 CtxSnapshot（纯值），Worker 内包装成 Ctx 方法对象。
 */

import { buildCtx } from './build-ctx';
import type { ProbeAction, WorkerInput, WorkerOutput } from './types';

self.onmessage = (ev: MessageEvent<WorkerInput>) => {
  const start = performance.now();
  const { script, snapshot } = ev.data;
  const actions: ProbeAction[] = [];
  const ctx = buildCtx(snapshot, actions);

  try {
    // eslint-disable-next-line @typescript-eslint/no-implied-eval
    const fn = new Function('ctx', script);
    const result = fn(ctx);
    const output: WorkerOutput = {
      ok: true,
      result,
      actions,
      durationMs: Math.round(performance.now() - start),
    };
    (self as unknown as Worker).postMessage(output);
  } catch (err: unknown) {
    const message =
      err instanceof Error ? (err.stack ?? err.message) : String(err);
    const output: WorkerOutput = {
      ok: false,
      stderr: message,
      durationMs: Math.round(performance.now() - start),
    };
    (self as unknown as Worker).postMessage(output);
  }
};

// 避免 TS 报 "isolatedModules" 错（无 import/export 时）
export {};
