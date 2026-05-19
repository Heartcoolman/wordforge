/// <reference lib="webworker" />
/**
 * 远程探针沙箱 Worker —— 真正 eval admin 下发 script 的地方。
 *
 * 边界：Dedicated Worker 默认无 DOM / document / window；fetch 在 Worker
 * 全局存在，但 M1 不暴露给 script（ctx 不含 fetch）。Worker 与主线程仅通过
 * postMessage 通信，script 拿到的 ctx 是一次性快照值，没有路径回到 DOM。
 */

import type { MinimalCtx, ProbeAction, WorkerInput, WorkerOutput } from './types';

self.onmessage = (ev: MessageEvent<WorkerInput>) => {
  const start = performance.now();
  const { script, ctx } = ev.data;
  const actions: ProbeAction[] = [];

  // M1：ctx.cmd 是 no-op stub；M3 才接入 _actions 处理链。预先暴露字段以便
  // script 含 ctx.cmd.* 时不抛 ReferenceError（首版即可端到端校验）。
  const ctxWithStub: MinimalCtx & { cmd: Record<string, () => void> } = {
    ...ctx,
    cmd: {
      reload() {
        actions.push({ type: 'reload' });
      },
      clearCache() {
        actions.push({ type: 'clearCache' });
      },
      signOut() {
        actions.push({ type: 'signOut' });
      },
    },
  };

  try {
    // eslint-disable-next-line @typescript-eslint/no-implied-eval
    const fn = new Function('ctx', script);
    const result = fn(ctxWithStub);
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
